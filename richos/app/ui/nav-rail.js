"use strict";
// THE NAVIGATION RAIL (design round 58, version A "Hairline"; the CEO's pick, 2026-11-06).
//
// His words: "The Codex desktop app has this navigation rail with icons. I think the RichOS app
// also needs such navigation rail for quick access to some things or areas." The items and what
// each opens are Urban's (richos-hq `docs/design/2026-11-06-navigation-rail-recommendation.md`),
// and NOTHING IS REBUILT: every item opens the surface that already exists.
//
//   Home         the home screen (`RichHome.show`), the same door as the RichOS wordmark. The
//                rail is not on the home screen: it lives in `#app`, which the home screen
//                covers and makes inert.
//   Chats        the app itself: companies, chats, the conversation. Selected while nothing
//                else on the rail is.
//   RichFinder   the search palette ⌘K opens (`RichSearch.open`). Selected while it is open.
//   RichCleaner  RichCleaner's own window (`cleaner_open_window`), in front. Never selected: it
//                is another window.
//   RichScribe   the RichScribe sheet Settings opens (`RichDictation.open`), with a gold dot
//                while a Mac permission is missing. Shown only where RichScribe is (the app says
//                so); selected while the sheet is open.
//   Bust a bug   the Bust a bug panel Settings opens, over the screen you are on; inert while a
//                report is open, exactly as the Settings row is. Selected while the panel is open.
//   resources    the live square (`resources.js` mounts it in `#nr-res`). Never selected.
//   your name    Settings, on the name field while no name is set (`main.js` paints it). Never
//                selected.
//
// Selected = the glyph in gold, a soft gold tile behind it and a 3 px gold bar at the rail's left
// edge that slides to it: the bar is a shape, so the state never rests on color alone. Every item
// is a button with its name for a screen reader, the selected one is `aria-current="page"`, a
// tooltip with its name (and shortcut) shows to its right on hover and keyboard focus, and the
// arrow keys move up and down the rail.
(function () {
  const nav = document.getElementById("nav-rail");
  if (!nav) return;
  const tip = document.getElementById("nr-tip");
  const ind = nav.querySelector(".nr-ind");
  const item = (k) => document.getElementById("nr-" + k);
  const reduced = () => window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  // ---- pages ------------------------------------------------------------------------------------
  // A page is a place on the rail that replaces the conversation in the main area. Chats is the
  // app as it always was; a feature that brings its own page registers it (`registerPage`).
  const pages = new Map();
  let page = "chats";
  function showPage(name) {
    if (name !== "chats" && !pages.has(name)) return;
    if (name === page) return paint();
    const was = pages.get(page);
    page = name;
    if (was && was.hide) was.hide();
    const now = pages.get(page);
    if (now && now.show) now.show();
    document.body.dataset.page = page;
    paint();
  }

  // ---- what each item opens --------------------------------------------------------------------
  function bugBusy() {
    const s = window.RichSettings;
    return !!(s && s.bugBusy && s.bugBusy());
  }
  const OPEN = {
    home() {
      if (window.RichHome && window.RichHome.show) window.RichHome.show("nav-rail");
    },
    chats() {
      showPage("chats");
    },
    finder() {
      if (window.RichSearch) window.RichSearch.open();
    },
    cleaner() {
      if (window.RichCleaner && window.RichCleaner.library) {
        Promise.resolve(window.RichCleaner.library()).catch((e) => {
          if (window.RichSettings && window.RichSettings.toast) window.RichSettings.toast(String(e));
        });
      }
    },
    scribe() {
      if (window.RichDictation) window.RichDictation.open();
    },
    bug() {
      if (bugBusy()) return;
      if (window.RichSettings && window.RichSettings.bustABug) window.RichSettings.bustABug();
    },
    me() {
      if (!window.RichSettings) return;
      if (item("me").classList.contains("is-unset")) {
        window.RichSettings.openMenu({ focus: "#user-name-input", call: true });
      } else {
        window.RichSettings.openMenu();
      }
    },
  };

  /// A press on the rail goes where the item goes, so a sheet or palette open in the content area
  /// steps aside first, each by its own declared way out (`RichDismiss.dismiss`: the control a
  /// `control:` declaration names, or the surface's own close for an `escape` one: the palette,
  /// Corrections, Feedback). The Bust a bug panel stays (it is beside the screen, not over it,
  /// and holds a report), and so does Settings, which closes on any press outside itself.
  /// Returns null while a first-run question owns the window (the rail then does nothing), else
  /// how many surfaces it closed.
  function clearForRail(item) {
    const D = window.RichDismiss;
    if (D && D.question && D.question()) return null;
    // Bust a bug reports the screen as it is, Settings opens over anything, and RichCleaner is
    // another window: none of them is a place, so nothing steps aside for them.
    if (item === "bug" || item === "me" || item === "cleaner") return 0;
    const keep = (o) => (item === "finder" && o.id === "search-overlay") || (item === "scribe" && o.id === "dictation-sheet");
    // Topmost first, one at a time, as Escape would: a surface refuses to close while another
    // covers it, so stacked sheets (the palette under RichScribe) close from the top down.
    const open = () => {
      const all = D && D.open ? D.open() : Array.from(document.querySelectorAll(".overlay:not([hidden])"));
      return all.filter((o) => o.classList.contains("overlay") && !o.hidden && !keep(o));
    };
    let closed = 0;
    for (let tries = 0; tries < 12; tries++) {
      const top = open()[0];
      if (!top) break;
      if (D && D.dismiss) D.dismiss(top);
      else top.hidden = true;
      if (!top.hidden) break;
      closed++;
    }
    return closed;
  }

  nav.addEventListener("click", (e) => {
    const b = e.target.closest ? e.target.closest(".nr-item[data-item]") : null;
    if (!b || !nav.contains(b)) return;
    hideTip();
    if (b.getAttribute("aria-disabled") === "true") return;
    const closed = clearForRail(b.dataset.item);
    if (closed === null) return;
    const go = OPEN[b.dataset.item] || (pages.has(b.dataset.item) ? () => showPage(b.dataset.item) : null);
    if (!go) return;
    // A closed surface may still hand focus back to its opener a moment later (the RichScribe
    // sheet's close returns it to Settings in a microtask). The destination opens after that,
    // so the keyboard lands where the press went.
    if (closed) setTimeout(go, 0);
    else go();
  });

  // ---- what is selected -------------------------------------------------------------------------
  const isUp = (id) => {
    const n = document.getElementById(id);
    return !!(n && !n.hidden && n.isConnected);
  };
  function selected() {
    if (isUp("bugdock")) return "bug";
    if (isUp("search-overlay")) return "finder";
    if (isUp("dictation-sheet")) return "scribe";
    return page;
  }

  let laidOut = false;
  function paint() {
    const sel = selected();
    for (const b of nav.querySelectorAll(".nr-item[data-item]")) {
      const on = b.dataset.item === sel;
      b.classList.toggle("is-sel", on);
      if (on) b.setAttribute("aria-current", "page");
      else b.removeAttribute("aria-current");
    }
    const bug = item("bug");
    if (bug) {
      if (bugBusy()) bug.setAttribute("aria-disabled", "true");
      else bug.removeAttribute("aria-disabled");
    }
    moveIndicator(sel);
  }

  function moveIndicator(sel) {
    const b = sel ? nav.querySelector('.nr-item[data-item="' + sel + '"]') : null;
    if (!b || b.hidden || !b.getClientRects().length) {
      ind.classList.remove("is-on");
      return;
    }
    const nr = nav.getBoundingClientRect();
    const br = b.getBoundingClientRect();
    const y = Math.round(br.top - nr.top + (br.height - ind.offsetHeight) / 2);
    // The first placement, and every one while the bar is off, is not a move: no slide.
    if (!laidOut || !ind.classList.contains("is-on") || reduced()) {
      ind.style.transition = "none";
      ind.style.transform = "translateY(" + y + "px)";
      void ind.offsetWidth;
      ind.style.transition = "";
      laidOut = true;
    } else {
      ind.style.transform = "translateY(" + y + "px)";
    }
    ind.classList.add("is-on");
  }

  // One paint per frame however many surfaces open or close in it.
  let queued = false;
  function schedule() {
    if (queued) return;
    queued = true;
    requestAnimationFrame(() => {
      queued = false;
      paint();
    });
  }
  // The surfaces the rail selects (the palette, the RichScribe sheet, the bug panel) show and
  // hide by their `hidden` attribute, so that one attribute on those three is what the rail
  // watches. Every other `hidden` in the app is ignored without a layout read, so a streaming
  // conversation never pays for the rail.
  const WATCHED = new Set(["bugdock", "search-overlay", "dictation-sheet"]);
  new MutationObserver((records) => {
    for (const r of records) {
      if (WATCHED.has(r.target.id)) return schedule();
    }
  }).observe(document.body, {
    subtree: true,
    attributes: true,
    attributeFilter: ["hidden"],
  });
  new ResizeObserver(schedule).observe(nav);

  // ---- the tooltip -----------------------------------------------------------------------------
  let tipFor = null;
  function showTip(b) {
    if (!tip || !b || !b.dataset.tip) return;
    tipFor = b;
    tip.textContent = b.dataset.tip;
    if (b.dataset.kbd) {
      const k = document.createElement("kbd");
      k.textContent = b.dataset.kbd;
      tip.appendChild(k);
    }
    tip.hidden = false;
    const r = b.getBoundingClientRect();
    tip.style.left = Math.round(r.right + 12) + "px";
    tip.style.top = Math.round(r.top + r.height / 2 - tip.offsetHeight / 2) + "px";
    b.setAttribute("aria-describedby", "nr-tip");
    void tip.offsetWidth;
    tip.classList.add("is-on");
  }
  function hideTip() {
    if (!tip) return;
    if (tipFor) tipFor.removeAttribute("aria-describedby");
    tipFor = null;
    tip.classList.remove("is-on");
    tip.hidden = true;
  }
  const tipTarget = (e) => {
    const b = e.target && e.target.closest ? e.target.closest(".nr-item[data-tip]") : null;
    return b && nav.contains(b) ? b : null;
  };
  nav.addEventListener("pointerover", (e) => {
    const b = tipTarget(e);
    if (b && b !== tipFor) showTip(b);
  });
  nav.addEventListener("pointerout", (e) => {
    const b = tipTarget(e);
    if (b && !b.contains(e.relatedTarget) && b !== document.activeElement) hideTip();
  });
  nav.addEventListener("focusin", (e) => {
    const b = tipTarget(e);
    if (b && b.matches(":focus-visible")) showTip(b);
  });
  nav.addEventListener("focusout", hideTip);
  window.addEventListener("blur", hideTip);

  // ---- the arrow keys --------------------------------------------------------------------------
  // The rail is one landmark; Up and Down move along it, wrapping. On the resources square Down
  // is its own (the Ink/Readout menu), which `resources.js` answers before this sees it.
  nav.addEventListener("keydown", (e) => {
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    const list = Array.from(nav.querySelectorAll(".nr-item, #res-square")).filter((n) => !n.hidden && n.getClientRects().length);
    const at = list.indexOf(document.activeElement);
    if (at < 0) return;
    e.preventDefault();
    list[(at + (e.key === "ArrowDown" ? 1 : list.length - 1)) % list.length].focus();
  });

  // ---- RichScribe's presence and its dot -------------------------------------------------------
  // `dictation.js` says whether RichScribe is in this build and whether it waits on a Mac
  // permission; an item that opens nothing is never shown.
  function setScribe(state) {
    const b = item("scribe");
    if (!b) return;
    const available = !!(state && state.available);
    const needs = available && !!state.needsPermission;
    b.hidden = !available;
    b.querySelector(".nr-dot").hidden = !needs;
    b.setAttribute("aria-label", needs ? "RichScribe, needs a Mac permission" : "RichScribe");
    b.dataset.tip = needs ? "RichScribe · needs a permission" : "RichScribe";
    if (tipFor === b) showTip(b);
    schedule();
  }

  // ---- your name -------------------------------------------------------------------------------
  /// `main.js` hands over the identity it reads (`get_user_identity`): initials in a filled
  /// circle, or with no name a plain person outline, never invented letters.
  function setIdentity(who) {
    const b = item("me");
    if (!b) return;
    const name = who && who.name && who.name.trim() ? who.name.trim() : "";
    b.classList.toggle("is-unset", !name);
    document.getElementById("nr-initials").textContent = name ? who.initials || "" : "";
    b.setAttribute("aria-label", name ? name + ", open Settings" : "Set your name, open Settings");
    b.dataset.tip = name || "Set your name";
    if (tipFor === b) showTip(b);
  }

  window.RichNavRail = {
    paint: schedule,
    setScribe,
    setIdentity,
    showPage,
    get page() {
      return page;
    },
    get selected() {
      return selected();
    },
    /// A feature's own page (`{ show(), hide() }`), reached from its rail item.
    registerPage(name, host) {
      pages.set(name, host || {});
    },
  };

  document.body.dataset.page = page;
  paint();
})();

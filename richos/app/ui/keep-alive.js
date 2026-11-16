// KEEP CHAT ALIVE (CEO, 2026-11-16; rounds 81 to 83).
//
//   "in long-running threads in the RichOS app, the user could have a toggle or a checkbox labeled
//    along the lines of "keep chat alive" or something like that. I mean, this checkbox would be in
//    every chat but off by default and in long-running threads where the user knows that he'll be
//    back the next morning, he could just check that checkbox to keep the cache for that particular
//    thread."
//   "The user will keep their Mac mini running 24/7. And on a laptop, if they close the laptop, they
//    wouldn't expect to have there anything running."
//   "There also needs to be one of those info icons (like we have in settings) that on hover shows
//    what that checkbox actually does and what it is for."
//
// In every chat's header, right of the chat's name: a checkbox "Keep chat alive" and an ⓘ. Off in
// every chat until that chat's own box is checked; each chat keeps its own (`nav.json`,
// `keep_alive_threads`, read with `get_keep_chat_alive` and saved with `set_keep_chat_alive`). The
// ⓘ is Settings' own (`.cx-info` / `.cx-tip`, the Codex review row's), with the same behavior:
// hover (with a short grace so the pointer can move into the words), keyboard focus, a click pins
// it, Escape or a press elsewhere puts it away. Neither the ⓘ nor its words flip the checkbox.
//
// What a checked box does is the app's, not this file's: while a chat is kept, each of its two
// sessions is sent one keep-alive message once it has been idle for 55 minutes (`richos-core`
// `chat_cache.rs`). This file only shows the box and saves the choice.
//
// The quota words' figures: one lapse (the whole chat read again) costs as much as 40 keep-alives
// on Opus and 80 on Fable, one keep-alive every 55 minutes (richos-hq
// `docs/research/2026-11-16-cache-heartbeat-what-can-be-done.md`): 40 x 55 min is about 37 hours,
// 80 x 55 min about 73 hours.
"use strict";

(function () {
  var Bridge = window.RichBridge;
  if (!Bridge) return;

  var INFO =
    '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true">' +
    '<circle cx="12" cy="12" r="9.25"/><path d="M12 11v5.5"/><circle cx="12" cy="7.6" r=".6" fill="currentColor"/></svg>';

  var kept = {}; // thread id -> whether its box is checked, as last read or chosen
  var current = null; // the thread on screen
  var root, box, info, tip;
  var t = { pinned: false, over: false, timer: null };

  function build() {
    root = document.createElement("div");
    root.id = "kca";
    root.className = "kca";
    root.hidden = true;
    root.innerHTML =
      '<label class="srch-check kca-check" for="kca-box">' +
      '<input type="checkbox" id="kca-box" aria-describedby="kca-tip" />' +
      '<span class="kca-word" data-w="Keep chat alive">Keep chat alive</span>' +
      "</label>" +
      '<span class="cx-info-wrap kca-info-wrap" id="kca-info-wrap">' +
      '<button class="cx-info kca-info" id="kca-info" type="button" aria-label="About Keep chat alive" ' +
      'aria-describedby="kca-tip" aria-expanded="false">' + INFO + "</button></span>" +
      '<div class="cx-tip kca-tip" id="kca-tip" role="tooltip" hidden>' +
      '<p class="kca-tip-head">Keep chat alive</p>' +
      '<p class="cx-p">Keeps this chat ready while you’re away. Come back hours later (or the next ' +
      "morning) and Rich answers right away.</p>" +
      '<div class="cx-aware"><p><strong>It saves some of your Claude Code quota.</strong> Picking up a ready ' +
      "chat costs less than reading the whole chat again. <strong>Meant for long chats you’ll come back " +
      "to within about a day.</strong> Keeping a chat ready costs a little every hour. If you stay away " +
      "longer than a day and a half when RichOS is on Opus (or 3 days when it’s on Fable), it costs " +
      "more than it saves.</p>" +
      '<p class="cx-unseen"><strong>Only while your Mac is on.</strong> A Mac that stays on keeps the chat ' +
      "ready all night. A closed laptop is asleep, so nothing runs.</p></div>" +
      "</div>";
    box = root.querySelector("#kca-box");
    info = root.querySelector("#kca-info");
    tip = root.querySelector("#kca-tip");

    // The chat's name and its checkbox share one lead group, so that where the header is narrow
    // (the context meter on, the output panel open, a small window) the checkbox can stand under
    // the name instead of squeezing it.
    var header = document.getElementById("stage-header");
    var crumb = document.getElementById("scope-crumb");
    var lead = document.createElement("div");
    lead.className = "kca-lead";
    header.insertBefore(lead, crumb);
    lead.appendChild(crumb);
    lead.appendChild(root);

    // The box moves at once and is put back if the choice could not be saved.
    box.addEventListener("change", function () {
      if (!current) return;
      var id = current;
      var next = box.checked;
      kept[id] = next;
      paint(true);
      Promise.resolve(Bridge.invoke("set_keep_chat_alive", { threadId: id, kept: next })).catch(function () {
        kept[id] = !next;
        if (id === current) paint(false);
        if (window.RichSettings && window.RichSettings.toast) {
          window.RichSettings.toast("That could not be saved, so Keep chat alive is as it was.");
        }
      });
    });

    // the ⓘ: Settings' behavior, word for word (codex-reviews.js)
    [info, tip].forEach(function (el) {
      el.addEventListener("mouseenter", function () {
        t.over = true;
        show();
      });
      el.addEventListener("mouseleave", function () {
        t.over = false;
        hide(false);
      });
    });
    info.addEventListener("focus", function () {
      show();
    });
    info.addEventListener("blur", function () {
      hide(false);
    });
    info.addEventListener("click", function () {
      t.pinned = !t.pinned || tip.hidden;
      if (t.pinned) show();
      else hide(false);
    });
    document.addEventListener(
      "pointerdown",
      function (e) {
        if (!tip.hidden && !(e.target.closest && e.target.closest("#kca-tip, #kca-info"))) {
          t.pinned = false;
          t.over = false;
          hide(true);
        }
      },
      true
    );
    window.addEventListener(
      "keydown",
      function (e) {
        if (e.key === "Escape" && !tip.hidden) {
          hide(true);
          e.stopPropagation();
        }
      },
      true
    );
  }

  function show() {
    clearTimeout(t.timer);
    if (tip.hidden) {
      tip.hidden = false;
      var tl = tip.getBoundingClientRect().left;
      var ir = info.getBoundingClientRect();
      tip.style.setProperty("--ax", Math.round(ir.left + ir.width / 2 - tl) + "px");
      void tip.offsetWidth;
    }
    tip.classList.add("is-shown");
    info.setAttribute("aria-expanded", "true");
  }
  function hide(now) {
    clearTimeout(t.timer);
    var go = function () {
      if (!now && (t.pinned || t.over || document.activeElement === info)) return;
      t.pinned = false;
      tip.classList.remove("is-shown");
      tip.hidden = true;
      info.setAttribute("aria-expanded", "false");
    };
    if (now) go();
    else t.timer = setTimeout(go, 220);
  }

  function paint(fresh) {
    var on = !!(current && kept[current]);
    box.checked = on;
    root.classList.toggle("is-on", on);
    if (fresh && on) {
      // checked: one gold ring goes out from the box, once (none with reduced motion)
      root.classList.remove("kca-ping");
      void root.offsetWidth;
      root.classList.add("kca-ping");
    }
  }

  function visible() {
    var out = document.getElementById("out-top");
    // a chat is open exactly when its Output button is (output-panel.js keeps that true), and no
    // page has taken the stage over it (RichPass, RichFiles: `body.page-open`)
    return !!current && out && !out.hidden && !document.body.classList.contains("page-open");
  }
  function sync() {
    var v = visible();
    if (v && root.hidden) {
      root.hidden = false;
      root.classList.remove("kca-in");
      void root.offsetWidth;
      root.classList.add("kca-in");
    } else if (!v) {
      root.hidden = true;
      hide(true);
    }
    paint(false);
    layout();
  }

  // THE HEADER'S GIVE-WAY ORDER (round 82). The context meter keeps its own form at every width
  // (context-meter.css); what is around it gives way, first step that fits wins (keep-alive.css):
  // one row; one row without the header's Output (meter on only); the checkbox under the chat's
  // name; that without the header's Output (meter on only). "Fits" is measured: the chat's name
  // whole, and the header's shown pieces inside it and clear of one another.
  var header, laying = false;
  var STEPS_ON = [[false, false], [false, true], [true, false], [true, true]];
  var STEPS_OFF = [[false, false], [true, false]];
  function fits() {
    var crumb = document.getElementById("scope-crumb");
    var cut = function (el) {
      return !!el && el.scrollWidth > el.clientWidth + 1;
    };
    if (cut(crumb) || cut(document.getElementById("scope-entity")) || cut(document.getElementById("scope-thread"))) return false;
    var cs = getComputedStyle(header);
    var hr = header.getBoundingClientRect();
    var lo = hr.left + parseFloat(cs.paddingLeft) - 0.5;
    var hi = hr.right - parseFloat(cs.paddingRight) + 0.5;
    var lead = header.querySelector(".kca-lead");
    var boxes = [];
    Array.prototype.forEach.call(header.children, function (c) {
      if (c === lead) return;
      var r = c.getBoundingClientRect();
      if (r.width > 0 && getComputedStyle(c).position !== "fixed") boxes.push(r);
    });
    if (lead) {
      // the lead group may shrink: measure what it holds, the chat's name and its checkbox
      var right = Math.max(crumb.getBoundingClientRect().right, root.getBoundingClientRect().right);
      boxes.push({ left: lead.getBoundingClientRect().left, right: right });
    }
    boxes.sort(function (x, y) { return x.left - y.left; });
    for (var i = 0; i < boxes.length; i++) {
      if (boxes[i].left < lo || boxes[i].right > hi) return false;
      if (i && boxes[i].left < boxes[i - 1].right + 6) return false;
    }
    return true;
  }
  function layout() {
    // (a header no longer in the document: the document was replaced under this script)
    if (laying || !header || !header.isConnected) return;
    laying = true;
    var meter = document.querySelector(".ctx-meter");
    var steps = meter && !meter.hidden ? STEPS_ON : STEPS_OFF;
    for (var i = 0; i < steps.length; i++) {
      header.classList.toggle("kca-under", steps[i][0]);
      header.classList.toggle("kca-noout", steps[i][1]);
      if (root.hidden || fits()) break;
    }
    laying = false;
  }

  // The chat on screen, and whether its box is checked. An answer for a chat he has since left is
  // kept for that chat and never painted over the one on screen.
  function readThread() {
    Promise.resolve(Bridge.invoke("active_context"))
      .then(function (ctx) {
        var id = ctx && ctx.thread_id ? ctx.thread_id : null;
        if (id !== current) {
          current = id;
          hide(true);
        }
        sync();
        if (!id) return;
        return Promise.resolve(Bridge.invoke("get_keep_chat_alive", { threadId: id })).then(function (on) {
          kept[id] = on === true;
          if (id === current) paint(false);
        });
      })
      .catch(function () {});
  }

  function start() {
    if (!document.getElementById("stage-header") || !document.getElementById("out-top")) {
      return setTimeout(start, 50);
    }
    build();
    header = document.getElementById("stage-header");
    // anything that changes what the header holds or how wide it is: its own size (the window, the
    // sidebar, the output panel), the context meter coming and going or its figures changing, the
    // chat's name, the techy badge, and the fonts arriving
    new ResizeObserver(layout).observe(header);
    new MutationObserver(function () {
      requestAnimationFrame(layout);
    }).observe(header, { attributes: true, attributeFilter: ["hidden"], childList: true, subtree: true, characterData: true });
    if (document.fonts && document.fonts.ready) document.fonts.ready.then(layout);
    // the chat on screen: the header's crumb names it, so a change there is a thread change
    new MutationObserver(readThread).observe(document.getElementById("scope-crumb"), {
      childList: true,
      subtree: true,
      characterData: true,
    });
    new MutationObserver(sync).observe(document.getElementById("out-top"), {
      attributes: true,
      attributeFilter: ["hidden"],
    });
    readThread();
  }

  window.RichKeepAlive = {
    state: function () {
      return {
        thread: current,
        visible: !!root && !root.hidden,
        checked: !!box && box.checked,
        kept: Object.keys(kept).filter(function (id) { return kept[id]; }),
        tipOpen: !!tip && !tip.hidden,
        under: !!header && header.classList.contains("kca-under"),
        noOut: !!header && header.classList.contains("kca-noout"),
      };
    },
  };

  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", start);
  else start();
})();

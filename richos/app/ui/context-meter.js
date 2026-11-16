// THE CONTEXT METER (CEO, 2026-11-16; round 80, carried by round 83).
//
//   "adding a "context meter" switch (somewhere at the bottom of the settings, off by default).
//    Switching that on, would make the context window of Rich and Stu visible in each thread like
//    so: "50% 500k/1000k" That will allow me to see and confirm whether or not idle compaction is
//    actually switched off in the app (and keep an eye on it in case that ever changes for any
//    reason)."
//
// Two pieces:
//
//   1. THE SWITCH. "Context meter", the last setting in the first column of Settings ("How it
//      looks"), under Show SwearMeter. Off by default, kept across relaunches (`config.json`,
//      `show_context_meter`). `settings-button.js` builds the row; this file is its host.
//   2. THE METER. While the switch is on, every chat's header carries two readouts, right of the
//      chat's name and left of Output: "Rich 50% 500k/1000k" and "Team 21% 212k/1000k". "Rich" is
//      Rich talking with you; "Team" is Rich directing your team (the app never names the back
//      end, so these are the two halves the "Rich's model" setting already names). Hover or focus
//      opens a card with the exact figures.
//
// The figures are each session's real context size: the tokens in the context at its last model
// request and the model's window, read off Claude Code's own stream (`richos-core`
// `chat_cache.rs`, measured against Claude Code's own figure by
// `scripts/qa/context-figure-probe.py`). `chat_context` gives a chat's two as last read (kept
// across relaunches) and `rich://context-meter` gives each change as it happens. A session that
// has made no request yet has nothing in its context: it reads 0.
"use strict";

(function () {
  var Bridge = window.RichBridge;
  if (!Bridge) return;

  // The window the figures are drawn against until a session's first result names its own.
  var DEFAULT_WINDOW = 1000000;

  var on = false;
  var current = null; // the thread on screen
  var figures = {}; // thread id -> [[used, size], [used, size]] (Rich, Team)
  var shown = [null, null]; // what each readout shows right now (it eases toward the target)
  var meter, card;

  // ---- formatting: "50% 500k/1000k" -------------------------------------------------------
  function k(n) {
    return Math.round(n / 1000) + "k";
  }
  function full(n) {
    return Math.round(n).toLocaleString("en-US");
  }
  function side(raw) {
    var used = raw && typeof raw.used === "number" ? raw.used : 0;
    var size = raw && typeof raw.size === "number" && raw.size > 0 ? raw.size : DEFAULT_WINDOW;
    return [used, size];
  }
  function accept(payload) {
    if (!payload || !payload.threadId) return;
    figures[payload.threadId] = [side(payload.rich), side(payload.team)];
  }
  function target(id) {
    return figures[id] || [[0, DEFAULT_WINDOW], [0, DEFAULT_WINDOW]];
  }

  // ---- the meter in the chat's header ------------------------------------------------------
  function gauge() {
    var g = document.createElement("span");
    g.className = "cm-bar";
    g.setAttribute("aria-hidden", "true");
    var f = document.createElement("span");
    f.className = "cm-fill";
    g.appendChild(f);
    return g;
  }
  function readout(who) {
    var r = document.createElement("span");
    r.className = "cm-read";
    var name = document.createElement("span");
    name.className = "cm-who";
    name.textContent = who;
    var p = document.createElement("span");
    p.className = "cm-pct";
    var t = document.createElement("span");
    t.className = "cm-tok";
    r.appendChild(name);
    r.appendChild(gauge());
    r.appendChild(p);
    r.appendChild(t);
    return r;
  }

  function buildMeter() {
    meter = document.createElement("div");
    meter.id = "ctx-meter";
    meter.className = "ctx-meter";
    meter.setAttribute("role", "group");
    meter.tabIndex = 0;
    meter.setAttribute("aria-describedby", "ctx-card");
    meter.hidden = true;
    meter.appendChild(readout("Rich"));
    var sep = document.createElement("span");
    sep.className = "cm-sep";
    sep.setAttribute("aria-hidden", "true");
    meter.appendChild(sep);
    meter.appendChild(readout("Team"));

    card = document.createElement("div");
    card.id = "ctx-card";
    card.className = "ctx-card";
    card.setAttribute("role", "tooltip");
    card.hidden = true;
    card.innerHTML =
      '<p class="cc-title">Context window in this chat</p>' +
      '<div class="cc-row" data-i="0"><span class="cc-who">Rich, talking with you</span><span class="cc-fig"></span></div>' +
      '<div class="cc-row" data-i="1"><span class="cc-who">Rich, directing your team</span><span class="cc-fig"></span></div>' +
      '<p class="cc-note">Each number only grows as the chat goes on. If one ever drops by itself, the chat was compacted.</p>';
    meter.appendChild(card);

    var header = document.getElementById("stage-header");
    var out = document.getElementById("out-top");
    header.insertBefore(meter, out);

    var hideT = null;
    function showCard() {
      clearTimeout(hideT);
      paintCard();
      card.hidden = false;
    }
    function hideCard() {
      hideT = setTimeout(function () {
        card.hidden = true;
      }, 120);
    }
    meter.addEventListener("pointerenter", showCard);
    meter.addEventListener("pointerleave", hideCard);
    meter.addEventListener("focus", showCard);
    meter.addEventListener("blur", hideCard);
    meter.addEventListener("keydown", function (e) {
      if (e.key === "Escape" && !card.hidden) {
        card.hidden = true;
        e.stopPropagation();
      }
    });
  }

  function paintReadouts() {
    var reads = meter.querySelectorAll(".cm-read");
    var t = current ? target(current) : target(null);
    for (var i = 0; i < 2; i++) {
      var n = shown[i] || 0;
      var size = t[i][1];
      reads[i].querySelector(".cm-pct").textContent = Math.round((n / size) * 100) + "%";
      reads[i].querySelector(".cm-tok").textContent = k(n) + "/" + k(size);
      reads[i].querySelector(".cm-fill").style.width = Math.max(0, Math.min(100, (n / size) * 100)) + "%";
    }
    meter.setAttribute(
      "aria-label",
      "Context window. Rich, talking with you: " + Math.round((t[0][0] / t[0][1]) * 100) + "%, " + k(t[0][0]) + " of " + k(t[0][1]) +
        ". Rich, directing your team: " + Math.round((t[1][0] / t[1][1]) * 100) + "%, " + k(t[1][0]) + " of " + k(t[1][1]) + "."
    );
    if (!card.hidden) paintCard();
  }
  function paintCard() {
    var t = current ? target(current) : target(null);
    var rows = card.querySelectorAll(".cc-row");
    for (var i = 0; i < 2; i++) {
      rows[i].querySelector(".cc-fig").textContent = full(t[i][0]) + " of " + full(t[i][1]) + " tokens";
    }
  }

  // The readouts ease toward a new figure (a chat switch, a turn that grew the context) so the
  // change is seen, never a jump the eye misses. A chat switch is quick; growth is slower.
  var raf = 0;
  function ease(ms) {
    cancelAnimationFrame(raf);
    var from = shown.slice();
    var t = current ? target(current) : target(null);
    var to = [t[0][0], t[1][0]];
    var t0 = performance.now();
    if (window.matchMedia && matchMedia("(prefers-reduced-motion: reduce)").matches) ms = 0;
    function step(now) {
      var f = ms ? Math.min(1, (now - t0) / ms) : 1;
      var e = 1 - Math.pow(1 - f, 3);
      for (var i = 0; i < 2; i++) shown[i] = (from[i] || 0) + (to[i] - (from[i] || 0)) * e;
      paintReadouts();
      if (f < 1) raf = requestAnimationFrame(step);
    }
    if (!ms) step(t0);
    else raf = requestAnimationFrame(step);
  }

  function visible() {
    var out = document.getElementById("out-top");
    // a chat is open exactly when its Output button is (output-panel.js keeps that true), and no
    // page has taken the stage over it (RichPass, RichFiles: `body.page-open`)
    return on && !!current && out && !out.hidden && !document.body.classList.contains("page-open");
  }
  function sync(ms) {
    if (!meter) return;
    var v = visible();
    if (v && meter.hidden) {
      meter.hidden = false;
      meter.classList.remove("cm-in");
      void meter.offsetWidth;
      meter.classList.add("cm-in");
    } else if (!v) {
      meter.hidden = true;
      card.hidden = true;
    }
    if (v) ease(ms == null ? 450 : ms);
  }

  // The chat on screen, and its figures as last read. The answer for a chat he has since left
  // is kept for that chat and never drawn over the one on screen.
  function readThread() {
    Promise.resolve(Bridge.invoke("active_context"))
      .then(function (ctx) {
        var id = ctx && ctx.thread_id ? ctx.thread_id : null;
        var changed = id !== current;
        current = id;
        sync(changed ? 450 : 0);
        if (!id) return;
        return Promise.resolve(Bridge.invoke("chat_context", { threadId: id })).then(function (payload) {
          accept(payload);
          if (id === current) sync(changed ? 450 : 0);
        });
      })
      .catch(function () {});
  }

  // ---- the meter's narrower forms --------------------------------------------------------------
  // The header's inside width (its padding, and with it the settings button's lane, taken off)
  // decides the meter's form: under 900px the gauges go, under 780px the two readouts stand one
  // above the other, under 600px the header's Output gives way (context-meter.css). Measured
  // here and said as classes on the header, whose own layout stays exactly as it is without
  // the meter.
  function form() {
    var header = document.getElementById("stage-header");
    if (!header) return; // the document was replaced under this script: nothing to measure
    var cs = getComputedStyle(header);
    var inside = header.clientWidth - parseFloat(cs.paddingLeft) - parseFloat(cs.paddingRight);
    header.classList.toggle("cm-compact", inside < 900);
    header.classList.toggle("cm-stacked", inside < 780);
    header.classList.toggle("cm-tight", inside < 600);
  }

  // ---- wiring ---------------------------------------------------------------------------------
  function start() {
    if (!document.getElementById("stage-header") || !document.getElementById("out-top")) {
      return setTimeout(start, 50);
    }
    buildMeter();
    form();
    // its content box: the window, the sidebar, the output panel and the settings button's lane
    new ResizeObserver(form).observe(document.getElementById("stage-header"));
    // the chat on screen: the header's crumb names it, so a change there is a thread change
    var crumb = document.getElementById("scope-crumb");
    new MutationObserver(readThread).observe(crumb, { childList: true, subtree: true, characterData: true });
    new MutationObserver(function () {
      sync(0);
    }).observe(document.getElementById("out-top"), { attributes: true, attributeFilter: ["hidden"] });

    // a session's context changed (a request of a turn, or a turn's end)
    Bridge.listen("rich://context-meter", function (ev) {
      var p = (ev && ev.payload) || {};
      accept(p);
      if (p.threadId === current && visible()) ease(1400);
    });

    Promise.resolve(Bridge.invoke("get_context_meter"))
      .then(function (shownNow) {
        on = shownNow === true;
        if (window.RichSettings) window.RichSettings.paint();
        sync(0);
      })
      .catch(function () {});
    readThread();

    if (window.RichSettings && window.RichSettings.registerContextMeter) {
      window.RichSettings.registerContextMeter({
        read: function () {
          return on;
        },
        // The switch moves at once and is put back if the choice could not be saved.
        write: function (next) {
          var before = on;
          on = next === true;
          sync(650);
          return Promise.resolve(Bridge.invoke("set_context_meter", { show: on })).catch(function () {
            on = before;
            sync(0);
            if (window.RichSettings) {
              window.RichSettings.paint();
              window.RichSettings.toast("The context meter setting could not be saved.");
            }
          });
        },
      });
    }
  }

  window.RichContextMeter = {
    state: function () {
      return {
        on: on,
        thread: current,
        visible: !!meter && !meter.hidden,
        figures: current ? target(current) : null,
        // each readout as it reads on screen: "Rich 50% 500k/1000k"
        text: meter ? Array.prototype.map.call(meter.querySelectorAll(".cm-read"), function (r) {
          return Array.prototype.map.call(r.querySelectorAll(".cm-who, .cm-pct, .cm-tok"), function (part) {
            return part.textContent;
          }).join(" ");
        }) : [],
      };
    },
  };

  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", start);
  else start();
})();

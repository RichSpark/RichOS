// YOUR OPENINGS: the gallery of every opening this user has (round 74; the CEO's order of
// 2026-11-14: "1: yes", the gallery with Play and "Make it my opening"; "4: yes", "One of mine,
// a different one each day" as a second way to start).
//
// Reached from Settings ("Your openings", under the splash screen's sound switch). It covers
// the window whole, like the Token spend screen, and it is always dark, like the screens it
// shows.
//
//   Play                 runs that opening full screen, exactly as it ran: `RichSplash.replay`.
//                        A click or a key takes it away, the space key holds it, as at launch.
//   Make it my opening   RichOS starts with that one from now on (`set_opening_pick`; the pick
//                        for the next start is made when the app quits).
//   Save image           the finished frame with its name and date, drawn now by the shell and
//                        saved to the Downloads folder as a picture to post (`save_opening_image`).
//
// WHAT IT NEVER SHOWS: an opening that has not been earned, by name or by number. Each ladder
// that has more to give shows one sealed tile and nothing about what is behind it. He finds
// out he crossed a line by crossing it.
//
// WHAT IS HIS is the shell's answer (`openings_state`), asked each time the page opens, so the
// wall never shows less than he has; every setting he changes here is saved by the shell.
//
// Every way in has its way out: Back, and Escape.

"use strict";

(function () {
  var RO = window.RichOpenings;
  if (!RO) return;

  var page = null;
  var lastFocus = null;

  function el(tag, cls, text) {
    var n = document.createElement(tag);
    if (cls) n.className = cls;
    if (text !== undefined && text !== null) n.textContent = text;
    return n;
  }

  function invoke(cmd, args) {
    var bridge = window.RichBridge;
    if (!bridge || typeof bridge.invoke !== "function") return Promise.reject(new Error("no shell"));
    try {
      return Promise.resolve(bridge.invoke(cmd, args || {}));
    } catch (e) {
      return Promise.reject(e);
    }
  }

  /// What RichOS starts with right now, as an opening's id.
  function current() {
    var o = RO.current();
    return o ? o.id : "ruled-standard"; // the app's own rule from the third start on
  }
  function mode() {
    return RO.record().pick === RO.DAILY ? "daily" : "pick";
  }

  /// Ask the shell what is his now, and draw from the answer. A page with no shell keeps what
  /// it was given.
  function refresh() {
    return invoke("openings_state").then(
      function (r) {
        RO.update(r);
        fill();
      },
      function () {
        fill();
      }
    );
  }

  /// Save a setting, then read everything back, so what the page shows is what was saved.
  function save(cmd, args, said) {
    return invoke(cmd, args).then(
      function () {
        return refresh().then(function () {
          if (said) say(said);
        });
      },
      function (e) {
        say(typeof e === "string" && e ? e : "That could not be saved.");
        return refresh();
      }
    );
  }

  function setPick(pick, said) {
    return save("set_opening_pick", { pick: pick }, said);
  }

  /// THE SOUND SWITCH, for both places that show it (this page and the Settings row).
  function setSound(on) {
    on = !!on;
    var rec = RO.record();
    var was = rec.sound;
    rec.sound = on; // at once, so the two switches agree before the shell answers
    paintSound();
    if (on && !was) RO.sound.audition();
    return invoke("set_opening_sound", { on: on }).then(
      function () {},
      function () {
        rec.sound = was;
        paintSound();
        say("That could not be saved.");
      }
    );
  }
  function paintSound() {
    var on = RO.sound.on();
    var row = document.getElementById("splash-sound");
    if (row) row.checked = on;
    var mine = page && page.querySelector("#opg-sound");
    if (mine) mine.checked = on;
  }

  function card(o) {
    var c = el("article", "opg-card");
    c.dataset.opening = o.id;

    var poster = el("button", "opg-poster");
    poster.type = "button";
    poster.setAttribute("aria-label", "Play " + o.name);
    var img = el("img");
    img.src = "assets/openings/posters/" + o.id + ".jpg";
    img.alt = "";
    img.loading = "lazy";
    img.decoding = "async";
    poster.appendChild(img);
    var glyph = el("span", "opg-play-glyph");
    glyph.setAttribute("aria-hidden", "true");
    poster.appendChild(glyph);
    poster.appendChild(el("span", "opg-tag", ""));
    poster.addEventListener("click", function () { play(o, poster); });
    c.appendChild(poster);

    var head = el("div", "opg-head");
    head.appendChild(el("span", "opg-no", "No. " + RO.number(o)));
    head.appendChild(el("span", "opg-name", o.name));
    c.appendChild(head);
    var fig = RO.figureOf(o);
    var what = fig ? fig + " " + RO.captionOf(o) : o.family === "material" ? "Your opening, remade after " + o.after : "The Operating System for the AI-Enabled CEO";
    c.appendChild(el("p", "opg-what", what));
    c.appendChild(el("p", "opg-when", RO.provenance(o)));

    var acts = el("div", "opg-acts");
    var playBtn = el("button", "opg-btn opg-btn--play", "Play");
    playBtn.type = "button";
    playBtn.addEventListener("click", function () { play(o, playBtn); });
    acts.appendChild(playBtn);
    var mineBtn = el("button", "opg-btn opg-btn--mine", "Make it my opening");
    mineBtn.type = "button";
    mineBtn.addEventListener("click", function () {
      setPick(o.id, o.name + " is your opening now.");
    });
    acts.appendChild(mineBtn);
    var saveBtn = el("button", "opg-btn opg-btn--save", "Save image");
    saveBtn.type = "button";
    saveBtn.addEventListener("click", function () {
      if (saveBtn.disabled) return;
      saveBtn.disabled = true;
      say("Saving the image…");
      invoke("save_opening_image", { id: o.id }).then(
        function (file) {
          saveBtn.disabled = false;
          say("Saved to your Downloads folder as " + file + ".");
        },
        function (e) {
          saveBtn.disabled = false;
          say(typeof e === "string" && e ? e : "The image could not be saved.");
        }
      );
    });
    acts.appendChild(saveBtn);
    c.appendChild(acts);
    return c;
  }

  function sealed(family) {
    var c = el("article", "opg-card opg-card--sealed");
    var poster = el("div", "opg-poster opg-poster--sealed");
    var s = el("div", "opg-sealed-seal");
    s.appendChild(RO.seal("g-" + family));
    poster.appendChild(s);
    c.appendChild(poster);
    var head = el("div", "opg-head");
    head.appendChild(el("span", "opg-name", "Sealed"));
    c.appendChild(head);
    c.appendChild(el("p", "opg-what", family === "material" ? "It opens with more time with Rich." : "It opens when Rich has earned it."));
    return c;
  }

  function play(o, from) {
    if (!window.RichSplash || !window.RichSplash.replay) return;
    lastFocus = from || null;
    var ok = window.RichSplash.replay(o.id, function () {
      // Back where he was.
      if (page && !page.hidden && lastFocus && lastFocus.focus && lastFocus.isConnected) lastFocus.focus({ preventScroll: true });
    });
    if (!ok) say("That opening could not be played.");
  }

  var sayTimer = null;
  function say(text) {
    if (!page) return;
    var live = page.querySelector(".opg-live");
    live.textContent = text;
    live.classList.add("opg-live--on");
    clearTimeout(sayTimer);
    sayTimer = setTimeout(function () { live.classList.remove("opg-live--on"); }, 3200);
  }

  /// Everything that depends on a setting: which card is the opening, which mode is on.
  function paint() {
    if (!page) return;
    var cur = current();
    var daily = mode() === "daily";
    var cards = page.querySelectorAll(".opg-card[data-opening]");
    for (var i = 0; i < cards.length; i++) {
      var on = cards[i].dataset.opening === cur;
      cards[i].classList.toggle("opg-card--current", on);
      cards[i].querySelector(".opg-tag").textContent = on ? (daily ? "Opens RichOS today" : "Opens RichOS") : "";
      var b = cards[i].querySelector(".opg-btn--mine");
      b.disabled = on && !daily;
      b.textContent = on && !daily ? "Your opening" : "Make it my opening";
    }
    page.querySelector("#opg-mode-pick").checked = !daily;
    page.querySelector("#opg-mode-daily").checked = daily;
    paintSound();
  }

  /// The wall and the count, from what is his now. Called on every open and after every
  /// change, so an opening earned since the page was built is on it.
  function fill() {
    if (!page) return;
    var had = document.activeElement && page.contains(document.activeElement) ? document.activeElement : null;
    var hadOpening = had && had.closest(".opg-card") ? had.closest(".opg-card").dataset.opening : null;
    var hadClass = had ? (had.className.match(/opg-btn--\w+|opg-poster/) || [null])[0] : null;
    var n = RO.earned().length;
    page.querySelector(".opg-lede").textContent =
      "Every opening you have seen or earned, " + n + " so far. Play any of them or choose the one RichOS starts with.";
    var wall = page.querySelector(".opg-wall");
    var scroll = wall.scrollTop;
    wall.textContent = "";
    // The openings, by ladder, in the order they are numbered.
    ["house", "power", "jobs", "hours", "edition", "material"].forEach(function (family) {
      if (!RO.families[family]) return;
      var all = RO.catalog.filter(function (o) { return o.family === family; });
      var mine = all.filter(function (o) { return !!RO.earnedOn(o); });
      if (!mine.length && family === "house") return;
      var sec = el("section", "opg-section");
      sec.appendChild(el("h2", "opg-section-title", RO.families[family].title));
      var grid = el("div", "opg-grid");
      mine.forEach(function (o) { grid.appendChild(card(o)); });
      if (mine.length < all.length) grid.appendChild(sealed(family));
      sec.appendChild(grid);
      wall.appendChild(sec);
    });
    wall.scrollTop = scroll;
    var company = page.querySelector("#opg-company");
    if (document.activeElement !== company) company.value = RO.record().company || "";
    paint();
    // A press that rebuilt the wall keeps the keyboard where it was.
    if (hadOpening && hadClass) {
      var again = page.querySelector('.opg-card[data-opening="' + hadOpening + '"] .' + hadClass);
      if (again && !again.disabled) again.focus({ preventScroll: true });
      else page.querySelector(".opg-back").focus({ preventScroll: true });
    }
  }

  function build() {
    page = el("section", "opg-page");
    page.id = "openings-page";
    page.setAttribute("aria-labelledby", "opg-title");

    // The left column: what this is, and the three things he can set.
    var side = el("div", "opg-side");
    var back = el("button", "opg-back", "Back to RichOS");
    back.type = "button";
    back.addEventListener("click", close);
    side.appendChild(back);
    var h = el("h1", "opg-title", "Your openings");
    h.id = "opg-title";
    side.appendChild(h);
    side.appendChild(el("p", "opg-lede", ""));

    var set = el("div", "opg-set");
    set.appendChild(el("p", "opg-set-title", "RichOS starts with"));
    [["pick", "The one I pick"], ["daily", "One of mine, a different one each day"]].forEach(function (m) {
      var l = el("label", "opg-option");
      var r = el("input");
      r.type = "radio";
      r.name = "opg-mode";
      r.id = "opg-mode-" + m[0];
      r.addEventListener("change", function () {
        if (!r.checked) return;
        if (m[0] === "daily") setPick(RO.DAILY, "RichOS starts with one of yours, a different one each day.");
        else setPick(current(), RO.byId(current()).name + " is your opening now.");
      });
      l.appendChild(r);
      l.appendChild(el("span", null, m[1]));
      set.appendChild(l);
    });

    set.appendChild(el("p", "opg-set-title", "Sound"));
    var sl = el("label", "opg-option");
    var sc = el("input");
    sc.type = "checkbox";
    sc.id = "opg-sound";
    sc.addEventListener("change", function () { setSound(sc.checked); });
    sl.appendChild(sc);
    sl.appendChild(el("span", null, "Two quiet notes as the bar lands"));
    set.appendChild(sl);

    var cl = el("label", "opg-set-title", "Your company on the opening");
    cl.setAttribute("for", "opg-company");
    set.appendChild(cl);
    var ci = el("input", "opg-input");
    ci.type = "text";
    ci.id = "opg-company";
    ci.maxLength = 32;
    ci.autocomplete = "organization";
    ci.spellcheck = false;
    ci.placeholder = "Company name";
    ci.setAttribute("aria-describedby", "opg-company-hint");
    var companyTimer = null;
    var saveCompany = function () {
      clearTimeout(companyTimer);
      companyTimer = null;
      invoke("set_opening_company", { name: ci.value }).then(
        function () { RO.record().company = ci.value.trim() || null; },
        function () { say("That could not be saved."); }
      );
    };
    ci.addEventListener("input", function () {
      clearTimeout(companyTimer);
      companyTimer = setTimeout(saveCompany, 400);
    });
    ci.addEventListener("change", function () { if (companyTimer !== null) saveCompany(); });
    set.appendChild(ci);
    var hint = el("p", "opg-hint", "One line in gold under ours, on every opening. Leave it empty for none.");
    hint.id = "opg-company-hint";
    set.appendChild(hint);
    side.appendChild(set);

    var live = el("p", "opg-live");
    live.setAttribute("role", "status");
    side.appendChild(live);
    page.appendChild(side);
    page.appendChild(el("div", "opg-wall"));

    document.body.appendChild(page);
  }

  function onKey(e) {
    if (!page || page.hidden) return;
    if (window.RichSplash && window.RichSplash.isUp && window.RichSplash.isUp()) return; // the opening's own keys
    if (e.key === "Escape" && !e.metaKey && !e.ctrlKey && !e.altKey) {
      e.preventDefault();
      e.stopPropagation();
      close();
    }
  }

  var opener = null;
  function open() {
    if (!page) build();
    opener = document.activeElement;
    // It was opened from the Settings panel, which would otherwise stay up over this page.
    if (window.RichSettings && window.RichSettings.close) window.RichSettings.close(false);
    page.hidden = false;
    document.body.classList.add("openings-open");
    fill();
    page.querySelector(".opg-back").focus({ preventScroll: true });
    refresh();
  }
  function close() {
    if (!page) return;
    page.hidden = true;
    document.body.classList.remove("openings-open");
    // Back to the settings button: the control he opened Settings with, which is still there.
    var gear = document.getElementById("set-btn");
    if (gear && gear.focus) gear.focus({ preventScroll: true });
    else if (opener && opener.focus && opener.isConnected) opener.focus({ preventScroll: true });
  }

  window.addEventListener("keydown", onKey, true);
  window.RichOpeningsPage = {
    open: open,
    close: close,
    isOpen: function () { return !!page && !page.hidden; },
    setSound: setSound
  };

  var entry = document.getElementById("openings-open");
  if (entry) entry.addEventListener("click", open);
})();

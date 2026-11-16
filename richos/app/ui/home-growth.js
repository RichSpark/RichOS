// THE HOME SCREEN FROM THE FIRST LAUNCH TO A PICTURE WORTH POSTING (round 75, CEO 2026-11-14:
// "Yes, build it with the picked defaults").
//
// The design is `richos-hq/design/mockups/rounds/round-75` and the two documents it was built from,
// `docs/design/richos-loro-home-from-first-launch-to-brag-worthy-2026-11-14.md` ("the design",
// section numbers below) and `richos-loro-home-stage-names-and-words-2026-11-14.md` ("the words").
// This file is everything that design adds to the home screen of a user with a team:
//
//   2.1  one line under the first-run banner: "Move through it. Click a light."
//   2.2  the handover at the first hire: the example drains to its outline, his first light comes on
//   2.3  "See it finished": the example in full for ten seconds, named "Example" throughout
//   3.1  six stages at 1, 30, 100, 300, 1,000 and 2,500 memories, each switching on one piece
//   3.2  his lights in their finished color from stage 3
//   3.3  the distance line under the brand line, and its card
//   3.4  new lights arriving when he comes home, with how many and since when
//   3.5  "Watch it grow": his loro from the first light to today
//   3.6  a stage crossed: the piece switches on while he watches, one sentence, the offer to watch
//   4.3  the weekly line        4.4  the look ahead at the next stage
//
// `home.js` calls in at marked places; `home/field-engine.js` carries the levels this sets
// (`VARIANT.growth`). Not built, as the round says: what onboarding asks for (2.4), Rich's spoken
// mention (3.6, an instruction file's sentence), the website gallery (4.5), saving a video file.
//
// WHAT IT REMEMBERS, in `localStorage` under `richos-home-growth` (the screen's own state, as
// `accounts.js` keeps its banner's): that the first-run example was on this screen (`example`), that
// the handover has played (`handover`), the stage whose crossing he has been shown (`stage`), how
// many memories were up when he last looked and when (`look`), and the week whose line he has seen
// (`week`). A store that cannot be read plays nothing twice: every moment is skipped, the picture
// and the distance line stay.
//
// DATES. "Day N", the weekly line and the order lights arrive in come from each object's
// `createdDay`: a repository object's first commit (`home_repos.rs`), a loro record's first sighting
// (`home_field.rs`, `meta.dates`). Where the app began keeping dates after his records existed
// (`meta.dates.undated`), the day count is left out and the weekly line waits for a whole dated week.
//
// Everything on the screen is built with `textContent`.

"use strict";

window.RichHomeGrowth = (function () {
  // ---------------------------------------------------------------------------------------------
  // THE STAGES. 1 and 2,500 are the app's own thresholds; the four between are the design's.
  // The names are Set A with "Self-Made" for the last stage (the words, section 3, his pick).
  // ---------------------------------------------------------------------------------------------
  var AT = [1, 30, 100, 300, 1000, 2500];
  var NAMES = ["One Light", "The Threads", "First Color", "The River", "Full Color", "Self-Made"];
  /// The name inside the distance line: Set A's own names run lowercase there (the words, section 6);
  /// "Self-Made" keeps its capitals, as in "900 to Self-Made" (section 3).
  var IN_LINE = ["one light", "the threads", "first color", "the river", "full color", "Self-Made"];
  var GETS = ["the first light", "the lights are joined", "the first territory in its own color", "the river runs from the sources", "every territory in color, the core lit", "the example is gone"];
  var CROSSED = [null, "The lights are joined now.", "The first territory is in its own color.", "The river is running from the sources.", "Every territory is in color and the core is lit.", "The example is gone. Everything on this screen was earned."];
  var HANDOVER = "That was the example. This light is the first of the real ones and the shape behind it is where they are going.";
  var GROWS = "It grows from real work with Rich: every decision made, every source handed to Rich, every job a specialist finishes. Nothing in it is made up. Nothing in it is ever taken away.";
  var INVITE = "Move through it. Click a light.";

  /// "See it finished" holds the example up this long.
  var FINISHED_MS = 10000;
  /// More new lights than this arrive together, not one by one.
  var ONE_BY_ONE_MAX = 60;

  function stageOf(count) {
    var s = 0;
    for (var i = 0; i < AT.length; i++) if (count >= AT[i]) s = i + 1;
    return s;
  }
  var fmt = function (n) {
    return Number(n).toLocaleString("en-US");
  };
  var memories = function (n) {
    return fmt(n) + (n === 1 ? " memory" : " memories");
  };

  // ---------------------------------------------------------------------------------------------
  // WHAT THE SCREEN REMEMBERS.
  // ---------------------------------------------------------------------------------------------
  var KEY = "richos-home-growth";
  var storeOk = true;
  function load() {
    try {
      var v = JSON.parse(window.localStorage.getItem(KEY) || "{}");
      return v && typeof v === "object" ? v : {};
    } catch (e) {
      storeOk = false;
      return {};
    }
  }
  function save(patch) {
    if (!storeOk) return;
    try {
      var v = load();
      for (var k in patch) v[k] = patch[k];
      window.localStorage.setItem(KEY, JSON.stringify(v));
    } catch (e) {
      storeOk = false;
    }
  }

  /// "this morning", "yesterday", "Monday", "November 3": the last time he looked, the way a person
  /// would say it (the words, 5.4).
  function sinceWords(then, now) {
    var a = new Date(then);
    var b = new Date(now);
    var day = function (d) {
      return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
    };
    var days = Math.round((day(b) - day(a)) / 86400000);
    if (days <= 0) return a.getHours() < 12 ? "this morning" : a.getHours() < 18 ? "this afternoon" : "this evening";
    if (days === 1) return "yesterday";
    if (days < 7) return a.toLocaleDateString("en-US", { weekday: "long" });
    return a.toLocaleDateString("en-US", { month: "long", day: "numeric" });
  }
  /// This week's Monday, as a whole number of local days: the name of a week.
  function weekOf(now) {
    var d = new Date(now);
    var monday = new Date(d.getFullYear(), d.getMonth(), d.getDate() - ((d.getDay() + 6) % 7));
    return Math.round(monday.getTime() / 86400000);
  }

  // ---------------------------------------------------------------------------------------------
  // THIS PICTURE: set by `variant()` before the engine's first frame, once per picture drawn.
  // ---------------------------------------------------------------------------------------------
  var active = false; // a user with a team has a picture
  var N = 0; // his memories in it
  var dates = null; // { today, undated } in `createdDay`'s days, or null when the dataset has none
  var firstLane = null; // the territory that was his largest when the first color came
  var allLanes = [];
  var scene = ""; // "handover" | "crossed" | "home" | ""
  var newCount = 0; // with "home": how many arrived since he last looked
  var lookedAt = 0;
  var startStage = 0;
  var loadedAt = 0; // when this dataset was read: its `today` is that day's

  function levelsAt(stage) {
    return { threads: stage >= 2 ? 1 : 0, river: stage >= 4 ? 1 : 0, core: stage >= 5 ? 1 : 0, color: stage >= 3 ? 1 : 0, done: stage >= 6 ? 1 : 0 };
  }
  /// His territories whose strands are on at `stage`. Each is named twice: as the dataset names it
  /// and as `field-ghost.js` renames it in front of the example (`own-`).
  function lanesAt(stage) {
    var ids = stage >= 5 ? allLanes : stage >= 3 && firstLane ? [firstLane] : [];
    var out = [];
    ids.forEach(function (id) {
      out.push(id, "own-" + id);
    });
    return out;
  }

  /// `home.js` (`loadGhost`) calls this with the engine's constants and his dataset (or `null`) before
  /// the engine runs: the stage's pieces on the first frame, and where a moment starts.
  function variant(v, records) {
    resetScene();
    active = true;
    // before the engine measures the screen: the left column stands one line lower for the distance line
    if (root) root.classList.add("grow-on");
    var nodes = records && Array.isArray(records.nodes) ? records.nodes : [];
    N = nodes.length;
    var meta = (records && records.meta) || {};
    dates = null;
    if (meta.dates && typeof meta.dates.today === "number") dates = { today: meta.dates.today, undated: Number(meta.dates.undated) || 0 };
    else if (meta.source === "repositories" && typeof meta.horizonDays === "number") dates = { today: meta.horizonDays, undated: 0 };

    // the engine's own arrival order, for the first hundred
    var order = nodes.slice().sort(function (a, b) {
      return (a.arrivalOrder || 0) - (b.arrivalOrder || 0) || (a.createdDay || 0) - (b.createdDay || 0) || (b.significance || 0) - (a.significance || 0) || (b.degree || 0) - (a.degree || 0);
    });
    var tally = {};
    firstLane = null;
    allLanes = [];
    order.forEach(function (n, r) {
      if (!Object.prototype.hasOwnProperty.call(tally, n.domain)) {
        tally[n.domain] = 0;
        allLanes.push(n.domain);
      }
      if (r >= AT[2]) return;
      tally[n.domain]++;
      if (firstLane === null || tally[n.domain] > tally[firstLane]) firstLane = n.domain;
    });

    var st = load();
    var stage = stageOf(N);
    // THE TERRITORY THAT EARNED THE FIRST COLOR KEEPS IT: once stage 3 has been reached it is
    // written down, so records that arrive later (and may outrank the first hundred within a day)
    // never move the color to another territory. "Nothing in it is ever taken away."
    if (storeOk && typeof st.firstLane === "string" && allLanes.indexOf(st.firstLane) >= 0) firstLane = st.firstLane;
    else if (storeOk && stage >= 3 && firstLane !== null) save({ firstLane: firstLane });
    scene = "";
    newCount = 0;
    startStage = stage;
    var startCount = N;
    var levels = levelsAt(stage);
    if (storeOk && st.example === true && st.handover !== true) {
      scene = "handover";
      startStage = 0;
      startCount = 0;
      levels = levelsAt(0);
      levels.ex = 1; // the example in full, as he last saw it
    } else if (storeOk && typeof st.stage === "number" && stage > st.stage && stage >= 2) {
      scene = "crossed";
      startStage = st.stage;
      // from what he last saw, so every light since arrives; the last five where that is not known
      startCount = st.look && typeof st.look.count === "number" && st.look.count >= 0 && st.look.count < N ? st.look.count : Math.max(0, N - 5);
      levels = levelsAt(startStage);
    } else if (storeOk && st.look && typeof st.look.count === "number" && N > st.look.count) {
      scene = "home";
      newCount = N - st.look.count;
      lookedAt = Number(st.look.at) || 0;
      startCount = st.look.count;

    }
    // A moment that switches a piece on while he watches holds the levels from the first frame:
    // the count may already be past the threshold when the picture comes up (295 to 320).
    if (scene === "crossed" || scene === "handover") {
      hold = true;
      shownStage = startStage;
    }
    loadedAt = Date.now();
    v.growth = { levels: levels, lanes: lanesAt(startStage) };
    v.count = startCount;
  }

  /// The first-run example is on this screen, for a user with no team (`home.js`, `startField`).
  function exampleSeen() {
    if (load().example !== true) save({ example: true });
  }

  // ---------------------------------------------------------------------------------------------
  // THE CHROME (`home.js`, `build`).
  // ---------------------------------------------------------------------------------------------
  var root = null;
  var el = {};
  function make(tag, cls, text, attrs) {
    var e = document.createElement(tag);
    if (cls) e.className = cls;
    if (text) e.textContent = text;
    if (attrs) for (var k in attrs) e.setAttribute(k, attrs[k]);
    return e;
  }

  function build(r) {
    root = r;
    // 2.1: one line under the first-run banner's sentence. 2.3: the word "Example" over it and the
    // line that runs out under it, shown only while the example is lit. All three stand beside
    // the sentence's own element, which stays the sentence alone; the column under the banner is
    // pushed down by the last of them that is showing (`home.js`, `reserveNoteInset`).
    var line = r.querySelector("#home-note-line");
    if (line) {
      el.exampleWord = make("p", "grow-example-word", "Example");
      line.parentNode.insertBefore(el.exampleWord, line);
      el.invite = make("p", "grow-invite", INVITE);
      line.parentNode.appendChild(el.invite);
      el.exampleBar = make("div", "grow-example-bar");
      line.parentNode.appendChild(el.exampleBar);
    }

    // 3.3: the distance line, under the brand line, and its card
    var brand = r.querySelector("#home-brand");
    el.distance = make("button", "grow-distance", "", { id: "home-distance", type: "button", "aria-expanded": "false", "aria-controls": "home-growth-card" });
    el.distance.hidden = true;
    brand.appendChild(el.distance);
    // Escape closes it (`main.js`, `data-dismiss`; on this screen the key is taken below first).
    el.card = make("div", "grow-card", "", { id: "home-growth-card", role: "dialog", "aria-label": "How this picture grows", "data-dismiss": "escape" });
    el.card.hidden = true;
    r.appendChild(el.card);
    el.distance.addEventListener("click", toggleCard);

    // 2.3 and 3.5: the two presses, each on the legend row it acts on
    var rows = r.querySelectorAll("#home-legend .row");
    el.grow = make("button", "grow-press", "Watch it grow", { id: "home-grow", type: "button" });
    el.finished = make("button", "grow-press", "See it finished", { id: "home-finished", type: "button" });
    rows[0].appendChild(el.grow);
    rows[1].appendChild(el.finished);
    // the legend says what color means (3.2)
    var b = rows[0].querySelector(".tx b");
    if (b) b.textContent = "Yours, in gold and in color";
    // from 2,500 the example and its legend are gone; the replay stays
    el.growSolo = make("button", "grow-press grow-solo", "Watch it grow", { id: "home-grow-solo", type: "button" });
    r.appendChild(el.growSolo);
    el.grow.addEventListener("click", growPressed);
    el.growSolo.addEventListener("click", growPressed);
    el.finished.addEventListener("click", finishedPressed);

    // 2.2 and 3.6: the sentence under the core
    el.moment = make("div", "grow-moment", "", { id: "home-moment", role: "status" });
    el.momentTitle = make("p", "title");
    el.momentLine = make("p", "line");
    el.momentOffer = make("button", "grow-press offer", "Watch it grow", { id: "home-moment-grow", type: "button" });
    el.momentNext = make("p", "next");
    el.moment.appendChild(el.momentTitle);
    el.moment.appendChild(el.momentLine);
    el.moment.appendChild(el.momentOffer);
    el.moment.appendChild(el.momentNext);
    r.appendChild(el.moment);
    el.momentOffer.addEventListener("click", growPressed);

    // 3.4 and 4.3: the caption, where "Newest" stands
    el.caption = make("div", "grow-caption", "", { id: "home-caption", role: "status" });
    r.appendChild(el.caption);

    // 3.5: the replay's counter and its last frame
    el.replay = make("div", "grow-replay", "", { id: "home-replay", role: "status" });
    el.replayStage = make("div", "stage");
    el.replayLine = make("div", "line");
    el.replay.appendChild(el.replayStage);
    el.replay.appendChild(el.replayLine);
    r.appendChild(el.replay);
  }

  // ---- the distance line and the card ----------------------------------------------------------
  var cardCount = 0;
  function paintDistance(count) {
    if (!el.distance) return;
    var s = stageOf(count);
    el.distance.textContent = "";
    if (s >= 6) {
      // after the last stage the brand line's count stands alone (the words, 4.1)
      el.distance.hidden = true;
      closeCard();
      return;
    }
    el.distance.hidden = false;
    if (count === 0) {
      el.distance.appendChild(document.createTextNode("the first one is the first light"));
    } else {
      el.distance.appendChild(make("b", null, fmt(AT[s] - count)));
      el.distance.appendChild(document.createTextNode(" to " + IN_LINE[s]));
    }
    el.distance.appendChild(make("span", "chev", "›", { "aria-hidden": "true" }));
    if (!el.card.hidden) paintCard(count);
  }
  function paintCard(count) {
    var s = stageOf(count);
    var c = el.card;
    c.textContent = "";
    c.appendChild(make("h2", null, "How this picture grows"));
    var list = make("ol");
    for (var i = 0; i < 6; i++) {
      var reached = i < s;
      var next = i === s;
      var li = make("li", reached ? "reached" : next ? "next" : "ahead");
      li.appendChild(make("span", "mark", "", { "aria-hidden": "true" }));
      var tx = make("span", "tx");
      tx.appendChild(make("b", null, NAMES[i]));
      // a stage not reached shows its name and its count; the next one shows everything
      tx.appendChild(make("span", "what", reached ? " · " + memories(AT[i]) + " · " + GETS[i] : next ? " · at " + memories(AT[i]) + " · " + GETS[i] : " · at " + memories(AT[i])));
      li.appendChild(tx);
      list.appendChild(li);
    }
    c.appendChild(list);
    c.appendChild(make("p", "grows", GROWS));
  }
  function toggleCard() {
    if (!el.card.hidden) return void closeCard();
    if (el.distance.getAttribute("aria-expanded") === "true") el.distance.setAttribute("aria-expanded", "false"); // hidden from outside
    paintCard(cardCount);
    el.card.hidden = false;
    el.distance.setAttribute("aria-expanded", "true");
    el.card.style.top = Math.round(el.distance.getBoundingClientRect().bottom + 14) + "px";
  }
  function closeCard() {
    if (!el.card || el.card.hidden) return false;
    el.card.hidden = true;
    el.distance.setAttribute("aria-expanded", "false");
    return true;
  }

  // ---- the engine's levels ---------------------------------------------------------------------
  var lo = null;
  var shownStage = -1;
  function setStage(stage, tau) {
    if (!lo || !lo.growth) return;
    var x = lo.growth;
    var lv = levelsAt(stage);
    x.XT.tau = tau;
    for (var k in lv) x.XT[k] = lv[k];
    var lanes = lanesAt(stage);
    lo.domains.forEach(function (d, di) {
      x.laneOnT[di] = lanes.indexOf(d.id) >= 0 ? 1 : 0;
    });
    shownStage = stage;
  }

  /// Is the example in this picture at all? From 2,500 of his repository objects it is not loaded.
  function hasExample() {
    var meta = window.MATURE_LORO && window.MATURE_LORO.meta;
    return !!(meta && meta.ghost);
  }

  /// `home.js` calls this whenever the number of his arrived memories changes.
  var hold = false; // a moment is driving the levels
  function onCount(count, stats, api) {
    if (!active || !root || !api || !api.growth) return;
    lo = api;
    cardCount = count;
    paintDistance(count);
    var legendOn = api.ghostLevel > 0.004 && hasExample();
    el.growSolo.classList.toggle("on", !legendOn && (count > 0 || replaying));
    el.grow.disabled = el.growSolo.disabled = (!replaying && count === 0) || (api.entrance && api.entrance.state !== "done");
    if (!hold && stageOf(count) !== shownStage) setStage(stageOf(count), shownStage < 0 ? 0.01 : 0.8);
    placeMoment();
  }
  function placeMoment() {
    if (!lo || !lo.coreScreen || !el.moment) return;
    el.moment.style.top = Math.round(lo.coreScreen()[1] + 172) + "px";
  }

  // ---- the caption -----------------------------------------------------------------------------
  function caption(text) {
    if (!el.caption) return;
    el.caption.textContent = text || "";
    el.caption.classList.toggle("on", !!text);
    root.classList.toggle("grow-caption-on", !!text);
  }
  /// "Last week: 38 memories, 2 decisions. Reached The River." (4.3), or "" for a week with nothing
  /// or a week the dataset cannot date. Last week is the seven days before this week's Monday.
  function weeklyLine(now) {
    if (!lo || !dates || !N) return "";
    // the dataset's today is the day it was read; the app may have stayed open since
    var a = new Date(loadedAt);
    var b = new Date(now);
    var since = Math.round((new Date(b.getFullYear(), b.getMonth(), b.getDate()) - new Date(a.getFullYear(), a.getMonth(), a.getDate())) / 86400000);
    var monday = dates.today + Math.max(0, since) - ((b.getDay() + 6) % 7);
    var from = monday - 7;
    // objects first seen on the day dates began (`createdDay` 0) may be older than that day
    if (dates.undated > 0 && from <= 0) return "";
    var mem = 0;
    var dec = 0;
    var before = 0;
    for (var r = 0; r < N; r++) {
      var i = lo.arrival[r];
      var d = lo.D.createdDay[i];
      if (d < from) before++;
      else if (d < monday) {
        mem++;
        if (lo.nodes[i].type === "decision") dec++;
      }
    }
    if (!mem) return "";
    var line = "Last week: " + memories(mem);
    if (dec) line += ", " + fmt(dec) + (dec === 1 ? " decision" : " decisions");
    line += ".";
    var reached = stageOf(before + mem);
    if (reached > stageOf(before) && reached >= 2) line += " Reached " + NAMES[reached - 1] + ".";
    return line;
  }

  // ---- the sentence under the core -------------------------------------------------------------
  function moment(title, line, offer) {
    el.momentTitle.textContent = title || "";
    el.momentLine.textContent = line || "";
    el.momentOffer.hidden = !offer;
    el.momentNext.textContent = "";
    placeMoment();
    el.moment.classList.add("on");
    root.classList.add("grow-moment-on");
  }
  function momentOff() {
    if (!el.moment) return;
    el.moment.classList.remove("on");
    root.classList.remove("grow-moment-on");
  }

  // ---- timers a moment owns, so a skip or a second press can end it cleanly ----------------------
  var timers = [];
  function later(ms, fn) {
    var t = setTimeout(function () {
      timers.splice(timers.indexOf(t), 1);
      fn();
    }, ms);
    timers.push(t);
    return t;
  }
  function clearTimers() {
    timers.forEach(clearTimeout);
    timers = [];
  }

  // ---- 2.2 THE HANDOVER --------------------------------------------------------------------------
  // The example stands in full for 1.4 s, then eases to its dim floor with a time constant of 1 s
  // (3 s later 5% of the way is left: e^-3 = 0.0498); his first light comes on 2.5 s into that, and
  // the sentence 0.6 s after it. Any key or press jumps to the end.
  var handoverOn = false;
  function playHandover() {
    hold = true;
    handoverOn = true;
    root.classList.add("grow-moment-on"); // the line under the core waits for this one
    var x = lo.growth;
    later(1400, function () {
      x.XT.exTau = 1.0;
      x.XT.ex = -1;
    });
    later(3900, function () {
      if (N > 0) lo.arrive(1);
    });
    later(4500, endHandover);
  }
  function endHandover() {
    if (!handoverOn) return;
    handoverOn = false;
    clearTimers();
    lo.growth.XT.ex = -1;
    if (N > 0 && lo.COUNT < 1) lo.arrive(1);
    hold = false;
    save({ handover: true });
    if (N > 0) {
      moment("", HANDOVER, false);
      // the rest of what he already has arrives behind the first, in the order it was earned (2.4)
      if (N > 1) later(2600, function () {
        if (lo.COUNT < N) lo.setCount(N, true);
      });
    } else {
      // nothing of his yet: the screen's own line says so ("None of this light is yours yet.")
      root.classList.remove("grow-moment-on");
    }
  }

  // ---- 2.3 SEE IT FINISHED -----------------------------------------------------------------------
  var finishedOn = false;
  var finishedTimer = 0; // its own: ending the example cancels no other moment
  function finishedPressed() {
    if (replaying || !lo || !lo.growth) return;
    if (finishedOn) return void endFinished();
    if (handoverOn) endHandover();
    finishedOn = true;
    closeCard();
    var x = lo.growth;
    x.XT.exTau = 0.7;
    x.XT.ex = 1;
    root.classList.add("grow-finished");
    el.finished.textContent = "End the example";
    exampleBanner(true);
    finishedTimer = setTimeout(endFinished, FINISHED_MS);
  }
  function endFinished() {
    if (!finishedOn) return;
    finishedOn = false;
    clearTimeout(finishedTimer);
    if (lo && lo.growth) {
      lo.growth.XT.exTau = 1.0;
      lo.growth.XT.ex = -1;
    }
    root.classList.remove("grow-finished");
    el.finished.textContent = "See it finished";
    exampleBanner(false);
  }
  /// The first-run banner's sentence, with the word "Example", for as long as the example is lit.
  var exampleUp = false;
  function exampleBanner(on) {
    exampleUp = on;
    if (el.exampleBar) {
      el.exampleBar.classList.remove("run");
      if (on) {
        void el.exampleBar.offsetWidth;
        el.exampleBar.classList.add("run");
      }
    }
    if (window.RichHome && window.RichHome.refreshNote) window.RichHome.refreshNote();
  }

  // ---- 3.5 WATCH IT GROW -------------------------------------------------------------------------
  // Ten seconds under 100 memories, twelve under 1,000, fourteen from there, after half a second on
  // the empty picture; the last frame stands six seconds.
  var replaying = false;
  var lastRows = -1;
  var replayRaf = 0;
  /// "Day N · ": while the replay runs, the day of the light that just landed, counted from his
  /// first; on the last frame (`today`), today's. Left out when the dataset cannot vouch for days.
  function dayOf(stats, today) {
    if (!dates || dates.undated !== 0 || !stats || !(stats.days > 0)) return "";
    var days = today ? Math.max(stats.days, dates.today - lo.D.createdDay[lo.arrival[0]] + 1) : stats.days;
    return "Day " + fmt(days) + " · ";
  }
  function growPressed() {
    if (finishedOn) endFinished();
    if (replaying) return void endReplay(true);
    if (!lo || !lo.growth || N === 0 || (lo.entrance && lo.entrance.state !== "done")) return;
    if (handoverOn) endHandover();
    // a crossing in flight ends here: he has seen it, and the look ahead lets go of the example
    if (hold) save({ stage: stageOf(N) });
    lo.growth.XT.ex = -1;
    clearTimers();
    closeCard();
    momentOff();
    caption("");
    replaying = true;
    hold = true;
    root.classList.add("grow-replaying");
    el.grow.textContent = "Stop";
    el.growSolo.textContent = "Stop";
    el.growSolo.classList.add("on");
    el.replay.classList.remove("last");
    el.replay.classList.add("on");
    lo.setCount(0, false);
    setStage(0, 0.01);
    var dur = N < 100 ? 10000 : N < 1000 ? 12000 : 14000;
    var t0 = performance.now() + 500;
    var stage = 0;
    paintReplay(0, 0, null);
    var step = function (now) {
      if (!replaying) return;
      var p = Math.max(0, Math.min(1, (now - t0) / dur));
      // one light at a time at first, then as fast as the work came
      var want = Math.max(p > 0 ? 1 : 0, Math.round(N * Math.pow(p, 1.7)));
      // each light flashes as it lands while they come one or two at a time; when the work came
      // faster, one of each frame's lights flashes, so the picture keeps its colors
      if (want - lo.COUNT > 2) lo.setCount(want - 1, true);
      if (want > lo.COUNT) lo.arrive(want - lo.COUNT);
      var s = stageOf(lo.COUNT);
      if (s !== stage) {
        stage = s;
        setStage(s, 0.45);
      }
      paintReplay(lo.COUNT, s, lo.stats);
      if (p >= 1) return void lastFrame();
      replayRaf = requestAnimationFrame(step);
    };
    replayRaf = requestAnimationFrame(step);
  }
  function paintReplay(count, stage, stats) {
    el.replayStage.textContent = stage >= 2 ? NAMES[stage - 1] : "";
    el.replayLine.textContent = count === 0 ? "" : dayOf(stats) + memories(count);
  }
  function lastFrame() {
    // "Day 41 · 312 memories · The River": the stage name stands on the last frame (his pick)
    el.replayStage.textContent = "";
    el.replayLine.textContent = dayOf(lo.stats, true) + memories(N) + " · " + NAMES[Math.max(0, stageOf(N) - 1)];
    el.replay.classList.add("last");
    later(6000, function () {
      endReplay(false);
    });
  }
  function endReplay(jump) {
    if (!replaying) return;
    replaying = false;
    cancelAnimationFrame(replayRaf);
    clearTimers();
    if (lo.COUNT !== N) lo.setCount(N, false);
    hold = false;
    setStage(stageOf(N), jump ? 0.25 : 0.8);
    root.classList.remove("grow-replaying");
    el.replay.classList.remove("on");
    el.grow.textContent = "Watch it grow";
    el.growSolo.textContent = "Watch it grow";
    onCount(lo.COUNT, lo.stats, lo);
    window.dispatchEvent(new Event("resize"));
  }

  // ---- 3.6 A STAGE CROSSED, AND 4.4 THE LOOK AHEAD ------------------------------------------------
  // The last lights arrive at 0.7 s; at 1.7 s the new piece starts to switch on with a time constant
  // of 1 s (about three seconds), with the stage's name and sentence; from 6.2 s the next stage's
  // piece shows faintly for 2.2 s.
  function playCrossed() {
    var s = stageOf(N);
    hold = true;
    later(700, function () {
      if (lo.COUNT < N) lo.arrive(N - lo.COUNT);
    });
    later(1700, function () {
      setStage(s, 1.0);
      moment(NAMES[s - 1], CROSSED[s - 1], true);
      save({ stage: s });
    });
    if (s < 6) {
      later(6200, function () {
        lookAhead(s + 1, true);
      });
      later(8400, function () {
        lookAhead(s + 1, false);
        hold = false;
      });
    } else {
      later(5000, function () {
        hold = false;
      });
    }
  }
  /// The next stage's piece, faint, for two seconds.
  function lookAhead(next, on) {
    var x = lo.growth;
    x.XT.tau = 0.45;
    if (!on) {
      el.momentNext.textContent = "";
      x.XT.ex = -1;
      setStage(stageOf(N), 0.6);
      return;
    }
    el.momentNext.textContent = "Next: " + NAMES[next - 1] + ", at " + memories(AT[next - 1]);
    if (next === 3 || next === 5) {
      var lanes = lanesAt(next);
      lo.domains.forEach(function (d, di) {
        if (lanes.indexOf(d.id) >= 0) x.laneOnT[di] = Math.max(x.laneOnT[di], 0.45);
      });
      x.XT.color = 1;
      if (next === 5) x.XT.core = 0.5;
    }
    if (next === 4) x.XT.river = 0.4;
    if (next === 6) {
      x.XT.exTau = 0.5;
      x.XT.ex = 0;
      x.XT.done = 0.45;
    }
  }

  // ---- 3.4 COMING HOME ---------------------------------------------------------------------------
  // The new lights arrive one by one over at most 2.6 s, starting 0.9 s after the screen is up.
  function playHome() {
    var k = N - lo.COUNT;
    if (k <= 0) return;
    if (!el.caption.textContent) caption(fmt(newCount) + " new since " + sinceWords(lookedAt, Date.now()));
    if (k > ONE_BY_ONE_MAX) {
      return void later(900, function () {
        if (lo.COUNT < N) lo.setCount(N, true);
      });
    }
    var gap = Math.min(420, 2600 / k);
    for (var i = 0; i < k; i++) {
      later(900 + i * gap, function () {
        if (lo.COUNT < N) lo.arrive(1);
      });
    }
  }

  // ---------------------------------------------------------------------------------------------
  // ON ARRIVAL: wait for the picture, for the opening curtain to be gone and for the home screen to
  // be in front, then play. Nothing waits on it: the screen is usable throughout.
  // ---------------------------------------------------------------------------------------------
  function curtainUp() {
    try {
      var s = window.RichSplash && window.RichSplash.state;
      return !!(s && s.shown && !s.reason);
    } catch (e) {
      return false;
    }
  }
  var arrived = false;
  var arriveTimer = 0;
  function arrive() {
    arriveTimer = 0;
    if (arrived || !active || !lo || !lo.growth || window.__loro !== lo) return;
    if ((lo.entrance && lo.entrance.state !== "done") || curtainUp() || !(window.RichHome && window.RichHome.isOpen())) return void (arriveTimer = setTimeout(arrive, 120));
    arrived = true;
    onCount(lo.COUNT, lo.stats, lo);
    var now = Date.now();
    var st = load();
    if (scene !== "handover") weekly(now);
    else if (storeOk) save({ week: weekOf(now) });
    if (weeklyShown && !el.caption.textContent) caption(weeklyShown);
    // he has looked: what is up now is what "new since" is counted from next time
    // (a crossing's stage is kept when its sentence shows, `playCrossed`)
    var seen = { look: { count: N, at: now } };
    if (scene !== "crossed") seen.stage = stageOf(N);
    save(seen);
    // Rich is told of a stage reached (CEO 2026-11-14): the fact only, which stage and the count
    // (`main.rs::home_stage_reached`). What he says about it is his instruction file's.
    if ((scene === "crossed" || scene === "handover") && stageOf(N) >= 1) tellRich(stageOf(N));
    if (scene === "handover") return void playHandover();
    if (scene === "crossed") return void playCrossed();
    if (scene === "home") return void playHome();

  }
  function tellRich(stage) {
    var bridge = window.RichBridge;
    if (!bridge || typeof bridge.invoke !== "function") return;
    Promise.resolve()
      .then(function () {
        return bridge.invoke("home_stage_reached", { stage: stage, name: NAMES[stage - 1], count: N });
      })
      .catch(function () {
        /* a notice that could not be written costs Rich's mention, never the screen */
      });
  }

  /// 4.3: the first visit of a week carries last week's line; it is gone on the next visit.
  function weekly(now) {
    if (!storeOk) return;
    var week = weekOf(now);
    if (load().week === week) return;
    var line = weeklyLine(now);
    if (line) caption(line);
    weeklyShown = line;
    save({ week: week });
  }
  /// This visit's weekly line, so a picture drawn again during the visit (new records read
  /// behind the first answer) puts it back; cleared when he leaves.
  var weeklyShown = "";
  /// The home screen came back with its picture already up (`home.js`, `show`): a visit like any
  /// other, also for an app left running from one week into the next.
  function visited() {
    if (!active || !arrived || !lo || window.__loro !== lo || replaying) return;
    weekly(Date.now());
  }

  /// The engine is up (`home.js`, `onLoroReady`).
  function ready(api) {
    if (!active || !api || !api.growth) return;
    lo = api;
    if (!arrived && !arriveTimer) arrive();
  }

  /// A new picture is about to be drawn, or the screen was left: every moment ends where it stands.
  function resetScene() {
    clearTimers();
    if (arriveTimer) clearTimeout(arriveTimer);
    arriveTimer = 0;
    cancelAnimationFrame(replayRaf);
    arrived = false;
    hold = false;
    handoverOn = false;
    shownStage = -1;
    lo = null;
    if (!root) return;
    if (replaying) {
      replaying = false;
      root.classList.remove("grow-replaying");
      el.replay.classList.remove("on");
      el.grow.textContent = "Watch it grow";
      el.growSolo.textContent = "Watch it grow";
    }
    if (finishedOn) {
      finishedOn = false;
      clearTimeout(finishedTimer);
      root.classList.remove("grow-finished");
      el.finished.textContent = "See it finished";
      exampleBanner(false);
    }
    closeCard();
    momentOff();
    caption("");
  }

  /// The home screen was left (`home.js`, `hide`): a replay stops on today, the example goes back to
  /// its outline, and the sentence, the caption and the card are for that visit only.
  function left() {
    if (!active || !root) return;
    if (handoverOn) endHandover();
    if (replaying) endReplay(true);
    if (finishedOn) endFinished();
    if (hold && arrived && lo && lo.growth) {
      // a crossing in flight: the stage is where it was going
      clearTimers();
      hold = false;
      if (lo.COUNT < N) lo.setCount(N, false);
      lo.growth.XT.ex = -1;
      setStage(stageOf(N), 0.25);
      save({ stage: stageOf(N) });
    }
    closeCard();
    momentOff();
    caption("");
    weeklyShown = "";
  }

  // Any key or press ends the handover; Escape closes the card, ends the example, stops the replay.
  // Registered before `home.js`, in the capture phase, so its Escape is taken here first.
  document.addEventListener(
    "keydown",
    function (e) {
      if (!active || !root || !(window.RichHome && window.RichHome.isOpen())) return;
      if (handoverOn) return void endHandover();
      if (e.key !== "Escape") return;
      if (replaying) endReplay(true);
      else if (finishedOn) endFinished();
      else if (!closeCard()) return;
      e.preventDefault();
      e.stopPropagation();
    },
    true
  );
  document.addEventListener(
    "pointerdown",
    function (e) {
      if (!active || !root) return;
      if (handoverOn) endHandover();
      if (el.card && !el.card.hidden && !el.card.contains(e.target) && !el.distance.contains(e.target)) closeCard();
    },
    true
  );
  window.addEventListener("resize", placeMoment);

  /// Today counted from his first light ("Day 67"), or 0 where the dataset cannot vouch for days.
  /// For the name of a saved video (`home-video.js`).
  function day() {
    if (!lo || !N || !dates || dates.undated !== 0) return 0;
    return Math.max((lo.stats && lo.stats.days) || 0, dates.today - lo.D.createdDay[lo.arrival[0]] + 1);
  }

  return {
    build: build,
    variant: variant,
    /// "Watch it grow", pressed: starts the replay, or stops one that is running (`home-video.js`).
    grow: growPressed,
    day: day,
    onCount: onCount,
    ready: ready,
    left: left,
    visited: visited,
    /// Is "Watch it grow" running? A picture is not drawn again under it (`home.js`, `refreshRecords`).
    get replaying() {
      return replaying;
    },
    exampleSeen: exampleSeen,
    get exampleUp() {
      return exampleUp;
    },
    /// Is a moment changing the count on every frame with the left column's rows unchanged? Then the
    /// screen is not re-measured for every new number (`home.js`, `renderTeamColumn`).
    sameRows: function (n) {
      var same = replaying && n === lastRows;
      lastRows = n;
      return same;
    },
    stageOf: stageOf,
    AT: AT,
    NAMES: NAMES,
    KEY: KEY,
    sinceWords: sinceWords,
    state: function () {
      return { active: active, n: N, scene: scene, arrived: arrived, stage: shownStage, replaying: replaying, finished: finishedOn, handover: handoverOn, hold: hold, dates: dates, count: lo ? lo.COUNT : null };
    },
  };
})();

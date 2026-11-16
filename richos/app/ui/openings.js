// THE OPENINGS: everything round 74 adds to the opening screen, in one file (the CEO's order of
// 2026-11-14; richos-hq design/mockups/rounds/round-74).
//
// `splash.js` still draws the screen and still owns when it leaves. This file is what it asks,
// at five marked points (`ROUND 74` in `splash.js`), for the things round 74 adds:
//
//   1. WHICH OPENING        `pick()`      the user's own pick, or one of his each day
//   2. WHAT IS ON IT        `dress()`     the milestone number, his company's line, the seal,
//                                         and the line a saved image carries
//   3. THE LANDING          `landed()`    the bar lands 0.8 s before the screen's time is up,
//                                         so the flare, the sparks and the sound happen inside it
//   4. THE SOUND            `sound`       two notes; the second lands with the bar
//   5. THE HANDOVER         `handover()`  the landed bar becomes the gold edge of Talk to Rich
//
// THE CATALOG is `openings-catalog.js` (data, read by the shell too): every opening there is.
// An opening is a library entry (`base`) plus, for a milestone or an edition, a number and a
// line. Adding an opening is adding a row there.
//
// WHAT IS HIS comes from the shell, put in front of every page script as
// `window.__RICHOS_OPENINGS__` (`src-tauri/src/openings.rs` `page_record`): which openings he
// has and since when, and his three settings (his pick, the sound, his company's line). A page
// with no shell behind it (a plain browser) has no record: nothing is his, and nothing sounds.
//
// NOTHING HERE DELAYS THE APP. It awaits nothing and reads no file; a throw anywhere in
// `dress()` is caught by `splash.js` and the screen is drawn without the additions.
//
// ROUND 85 (the CEO, 2026-11-16; richos-hq design/mockups/rounds/round-85): NOTHING HERE ANSWERS
// THE POINTER. "The spotlight effect makes it look less premium and must be removed from all
// splash screens in general." The light that followed the pointer is gone, all three parts of
// it: the lamp that moved (and drifted when left alone), the warm pool on the mat, and the
// glint on the gold of both marks and the bar. The lamp is where `splash.css` draws it, at
// rest, on every opening. This file listens to nothing the user does.
//
// IT COSTS A FRAME NOTHING IT DID NOT COST BEFORE. Where things rest in the window is read
// once (`geometry()`, from layout offsets, which no stage still rising changes) and again only
// when the window is resized: no frame measures the layout. The sound file is asked for a
// second into the splash, never while the app is starting.
//
// EVERYTHING HERE STANDS AT A MOMENT. The shell draws the first second of the picked opening
// when the app quits, as pictures of this code held still at one moment after another in a page
// nobody sees, and the next start's page joins them where they have reached (`splash.js`, HELD
// AT A MOMENT). So what this file paints is a function of the splash's own clock (`paint(t)`)
// and of nothing else.

"use strict";

(function () {
  // ---------------------------------------------------------------------------------------
  // The catalog, and what is his
  // ---------------------------------------------------------------------------------------

  var data = window.RichOpeningsCatalog || {};
  // `rank` 0..1 is how far up its ladder an opening is; everything that escalates (how many
  // sparks, how bright the keyline, how many lights) reads it, so a rung added later takes its
  // place without a new effect being written.
  var CATALOG = Array.isArray(data.openings) ? data.openings : [];
  var FAMILIES = data.families || {};

  var DAILY = "daily"; // the stored pick that means "one of mine, a different one each day"

  // The CEO's answers of 2026-11-14: sound on by default with a switch in Settings, and the
  // "Two notes" sound. `two-note.wav` every day; `two-note-grand.wav` on a milestone or an
  // edition. Its second note is 130 ms into the file, so the file starts that long before the
  // bar lands (`sounds/build/synth.mjs` in the round wrote both; no samples, ours outright).
  var SOUND_FILE = "assets/sounds/two-note";
  var SOUND_LEAD_MS = 130;
  var LAND_EARLY_MS = 800;
  var HANDOVER_MS = 460;
  // The sound's file is asked for this long into the splash: after the launch, a second before
  // the earliest landing needs it (3000 - 800 - 130 = 2070).
  var SOUND_LOAD_AT_MS = 1000;

  /// The shell's record, as plain values this file may change when he changes a setting.
  function normalize(r) {
    r = r && typeof r === "object" ? r : {};
    var earned = {};
    if (r.earned && typeof r.earned === "object") {
      for (var k in r.earned) if (Object.prototype.hasOwnProperty.call(r.earned, k) && typeof r.earned[k] === "string") earned[k] = r.earned[k];
    }
    return {
      firstStart: typeof r.firstStart === "string" ? r.firstStart : null,
      version: typeof r.version === "string" ? r.version : "",
      earned: earned,
      pick: typeof r.pick === "string" && r.pick ? r.pick : null,
      sound: r.sound === true,
      company: typeof r.company === "string" && r.company ? r.company : null,
      day: typeof r.day === "number" && isFinite(r.day) ? Math.floor(r.day) : null,
      // With "a different one each day": the one the shell says today opens on.
      today: typeof r.today === "string" && r.today ? r.today : null,
      // The shell is still counting his spend for the first time (the gallery asks again).
      counting: r.counting === true
    };
  }
  var record = normalize(window.__RICHOS_OPENINGS__);

  function byId(id) {
    for (var i = 0; i < CATALOG.length; i++) if (CATALOG[i].id === id) return CATALOG[i];
    return null;
  }
  function number(o) {
    return CATALOG.indexOf(o) + 1;
  }
  function earnedOn(o) {
    return (o && record.earned[o.id]) || null;
  }
  function earnedList() {
    return CATALOG.filter(function (o) { return !!earnedOn(o); });
  }
  var MONTHS = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
  function longDate(iso) {
    var p = String(iso).split("-");
    return MONTHS[Number(p[1]) - 1] + " " + Number(p[2]) + ", " + p[0];
  }
  function figureOf(o) {
    return o.figure ? o.figure.replace("{version}", record.version || "") : null;
  }
  function captionOf(o) {
    return o.caption || (FAMILIES[o.family] && FAMILIES[o.family].caption) || "";
  }
  /// What the gallery and a saved image call it: "No. 7 · Struck".
  function title(o) {
    return o.name;
  }
  /// The second half of a saved image's line: when, or since when.
  function provenance(o) {
    var on = earnedOn(o);
    if (!on) return "";
    if (o.family === "house") return "Yours since " + longDate(on);
    return "Earned " + longDate(on);
  }

  // ---------------------------------------------------------------------------------------
  // The library: every opening becomes an entry `splash.js` can validate and draw
  // ---------------------------------------------------------------------------------------

  function entryId(o) {
    return "opening/" + o.id;
  }
  /// THE OPENINGS AS LIBRARY ENTRIES, in the catalog's order. `splash.js` `pool()` puts them
  /// after the library's own: the same list, in the same order, the shell builds
  /// (`splash_pick.rs` `catalog`), so its pick names an entry here. The library itself is never
  /// written to; it stays the data file it is.
  var built = null;
  function entries() {
    if (built) return built;
    var lib = window.RichSplashLibrary;
    if (!lib || !Array.isArray(lib.variations)) return [];
    built = [];
    var source = {};
    lib.variations.concat(window.RichSplashMaterials || []).forEach(function (v) { source[v.id] = v; });
    CATALOG.forEach(function (o) {
      var fam = FAMILIES[o.family];
      if (!fam) return;
      var from = source[o.base || fam.base];
      if (!from) return;
      var t = JSON.parse(JSON.stringify(from.tokens));
      if (fam.seconds) t.seconds = fam.seconds;
      if (o.figure && t.keylineWidth !== "0px") {
        // The milestone's frame: the keyline turns to gold, brighter with every rung.
        t.keylineColor = "rgba(194,163,92," + (0.3 + 0.45 * (o.rank || 0)).toFixed(2) + ")";
      }
      built.push({ id: entryId(o), name: o.name, source: from.source, tokens: t });
    });
    return built;
  }

  /// WHICH OPENING IS HIS PICK TODAY, as a catalog row, or null for the app's own rule. The
  /// same rule as the shell's `launch::pick_splash`, which is the one that decides a start;
  /// this is the page's fallback (`splash.js` `choose()`) and what the gallery marks.
  function current() {
    var stored = record.pick;
    if (stored === DAILY) {
      // A different one of his each day, never a dice roll: the same day always opens on the
      // same screen, and only on one he has. The shell says which (`openings::daily`: his
      // openings take turns, a later day on the one after the last); where it said nothing (a
      // plain browser), the calendar picks, which is also the shell's answer before the first.
      var told = record.today ? byId(record.today) : null;
      if (told && earnedOn(told)) return told;
      var mine = earnedList();
      if (!mine.length || record.day === null) return null;
      return mine[((record.day % mine.length) + mine.length) % mine.length];
    }
    var o = stored ? byId(stored) : null;
    return o && earnedOn(o) ? o : null;
  }
  /// `splash.js` `choose()` asks this first. Null means "your own rule": #1, then #2, then #1.
  function pick(candidates) {
    var o = current();
    if (!o) return null;
    for (var i = 0; i < candidates.length; i++) if (candidates[i].id === entryId(o)) return candidates[i];
    return null;
  }
  function openingOf(entry) {
    if (!entry) return null;
    if (entry.id.indexOf("opening/") === 0) return byId(entry.id.slice(8));
    if (entry.id === "round-11/v1") return CATALOG[0] || null;
    if (entry.id === "round-11/v2") return CATALOG[1] || null;
    return null;
  }

  // ---------------------------------------------------------------------------------------
  // The sound
  // ---------------------------------------------------------------------------------------

  var sound = {
    on: function () {
      return record.sound;
    },
    _el: null,
    /// Load the file ahead of the landing, so the notes are not late by a disk read.
    prepare: function (grand) {
      if (!record.sound || typeof Audio === "undefined") return false;
      try {
        var a = new Audio(SOUND_FILE + (grand ? "-grand" : "") + ".wav");
        a.preload = "auto";
        a.load();
        sound._el = a;
        return true;
      } catch (_e) {
        return false;
      }
    },
    play: function () {
      var a = sound._el;
      if (!a) return;
      sound._el = null;
      try {
        var p = a.play();
        // A page that may not play yet refuses, silently, and the screen is whole without it.
        if (p && p.catch) p.catch(function () {});
      } catch (_e) {
        /* same */
      }
    },
    /// The switch being turned on: the sound itself is the confirmation, once.
    audition: function () {
      try {
        var a = new Audio(SOUND_FILE + ".wav");
        var p = a.play();
        if (p && p.catch) p.catch(function () {});
        return a;
      } catch (_e) {
        return null;
      }
    }
  };

  // ---------------------------------------------------------------------------------------
  // Small builders
  // ---------------------------------------------------------------------------------------

  var SVG_NS = "http://www.w3.org/2000/svg";
  function el(tag, cls, text) {
    var n = document.createElement(tag);
    if (cls) n.className = cls;
    if (text !== undefined && text !== null) n.textContent = text;
    return n;
  }
  function svgEl(tag, attrs) {
    var n = document.createElementNS(SVG_NS, tag);
    for (var k in attrs) if (attrs.hasOwnProperty(k)) n.setAttribute(k, attrs[k]);
    return n;
  }
  function reduceMotion() {
    try {
      return window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    } catch (_e) {
      return false;
    }
  }
  function seeded(seed) {
    var s = seed >>> 0;
    return function () {
      s = (s + 0x6d2b79f5) >>> 0;
      var t = Math.imul(s ^ (s >>> 15), 1 | s);
      t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
      return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    };
  }

  // THE SEAL. An engine-turned rosette in struck gold with the arrow of the mark at its center:
  // the hallmark of a rare edition, and the face of an opening that is still sealed. Drawn, not
  // loaded, so it is sharp at any size and there is no file to ship.
  // The arrow is the mark's own path: `splash.js` hands it over (`RichOpenings.arrow`), so there
  // is one copy of the logo in the app, not two.
  function seal(uid) {
    var svg = svgEl("svg", { viewBox: "-60 -60 120 120", class: "ro-seal-svg", "aria-hidden": "true", focusable: "false" });
    var defs = svgEl("defs", {});
    var g = svgEl("radialGradient", { id: "ro-seal-g-" + uid, cx: "38%", cy: "32%", r: "80%" });
    [["0", "#F3E0A8"], ["0.45", "#C9A85C"], ["1", "#7C5F24"]].forEach(function (s) {
      g.appendChild(svgEl("stop", { offset: s[0], "stop-color": s[1] }));
    });
    defs.appendChild(g);
    svg.appendChild(defs);
    // The scalloped edge: 28 lobes.
    var edge = "";
    for (var i = 0; i <= 280; i++) {
      var a = (i / 280) * Math.PI * 2;
      var r = 55 + 2.4 * Math.cos(a * 28);
      edge += (i ? "L" : "M") + (r * Math.cos(a)).toFixed(2) + "," + (r * Math.sin(a)).toFixed(2);
    }
    svg.appendChild(svgEl("path", { d: edge + "Z", fill: "url(#ro-seal-g-" + uid + ")" }));
    // The engine turning: three rose curves, cut dark into the gold.
    [[12, 30, 13, 0.5], [9, 37, 7, 0.42], [16, 44, 4, 0.36]].forEach(function (c) {
      var d = "";
      for (var j = 0; j <= 720; j++) {
        var t = (j / 720) * Math.PI * 2;
        var rr = c[1] + c[2] * Math.cos(c[0] * t);
        d += (j ? "L" : "M") + (rr * Math.cos(t)).toFixed(2) + "," + (rr * Math.sin(t)).toFixed(2);
      }
      svg.appendChild(svgEl("path", { d: d + "Z", fill: "none", stroke: "#5A4316", "stroke-width": "0.5", opacity: String(c[3]) }));
    });
    svg.appendChild(svgEl("circle", { r: "50.5", fill: "none", stroke: "#5A4316", "stroke-width": "0.6", opacity: "0.55" }));
    svg.appendChild(svgEl("circle", { r: "19", fill: "#1A2440" }));
    svg.appendChild(svgEl("circle", { r: "19", fill: "none", stroke: "#F3E0A8", "stroke-width": "0.7", opacity: "0.7" }));
    var d = window.RichOpenings && window.RichOpenings.arrow;
    if (d) svg.appendChild(svgEl("path", { d: d, fill: "url(#ro-seal-g-" + uid + ")", transform: "translate(-11.3 -20.9) scale(0.0477)" }));
    return svg;
  }

  // ---------------------------------------------------------------------------------------
  // Dressing the screen
  // ---------------------------------------------------------------------------------------

  var live = null; // everything about the screen that is up right now

  /// `splash.js` `build()` calls this with the finished composition, before it is on screen.
  /// `api` is `{ elapsed(), held(), hold, silent }`.
  function dress(root, entry, api) {
    var o = openingOf(entry);
    if (!o) return;
    var fam = FAMILIES[o.family] || {};
    var plinth = root.querySelector(".splash-plinth");
    var foot = root.querySelector(".splash-foot");
    // A milestone or an edition carries a number. The update edition's is the version that is
    // running; where nobody said it (a picture made for the gallery), its line still stands.
    var numbered = !!o.figure;
    var figure = figureOf(o) || "";
    root.dataset.opening = o.id;
    root.dataset.family = o.family;
    root.style.setProperty("--ro-rank", String(o.rank || 0));

    live = {
      root: root, o: o, fam: fam, api: api, plinth: plinth,
      landAt: api.hold - LAND_EARLY_MS, landed: false, settled: false, soundDue: null, soundGrand: numbered,
      count: null, fx: null, bar: root.querySelector(".splash-bar"), geo: null
    };

    // 1. THE NUMBER, as the hero: the size of $50,000 Tally on every milestone.
    if (numbered) {
      root.classList.add("splash--figure");
      var wrap = el("div", "splash-rise ro-figure-wrap");
      var fig = el("div", "ro-figure");
      var m = /^(\$?)([\d,]+)(.*)$/.exec(figure);
      if (m && !o.still) {
        if (m[1]) fig.appendChild(el("span", "ro-figure-unit", m[1]));
        var target = Number(m[2].replace(/,/g, ""));
        if (fam.fx === "roll") {
          // HOURS: each digit rolls into place on its own wheel, the last one landing last.
          var digits = m[2].split("");
          digits.forEach(function (d, i) {
            var col = el("span", "ro-wheel");
            var strip = el("span", "ro-wheel-strip");
            var turns = 1 + i;
            var seq = [];
            for (var k = 0; k <= turns * 10 + Number(d); k++) seq.push(String(k % 10));
            strip.textContent = seq.join("\n");
            strip.style.setProperty("--ro-roll-to", "calc(" + -(seq.length - 1) + " * var(--ro-wheel))");
            strip.style.setProperty("animation-duration", (1.7 + i * 0.55) + "s");
            col.appendChild(strip);
            fig.appendChild(col);
          });
        } else {
          var num = el("span", "ro-figure-num", reduceMotion() ? m[2] : "0");
          fig.appendChild(num);
          live.count = { node: num, target: target, from: 600, to: Math.min(3000, live.landAt - 500), shown: null };
        }
        if (m[3]) fig.appendChild(el("span", "ro-figure-tail", m[3]));
      } else {
        fig.textContent = figure;
      }
      wrap.appendChild(fig);
      wrap.appendChild(el("div", "ro-caption", captionOf(o)));
      plinth.insertBefore(wrap, foot.parentNode);
    }

    // 2. HIS COMPANY'S LINE, struck in gold under ours. Optional; ours stays the largest.
    if (api.card && record.company) foot.appendChild(el("div", "ro-company", record.company));

    // 3. THE SEAL of a rare edition, on the corner of the plinth like a seal on a deed.
    if (fam.fx === "seal") {
      var s = el("div", "ro-seal");
      s.appendChild(seal("p"));
      s.style.setProperty("animation-delay", live.landAt + "ms");
      plinth.appendChild(s);
    }
    if (o.struck) {
      root.classList.add("splash--struck");
      plinth.style.setProperty("--ro-land", live.landAt + "ms");
    }

    // 4. THE LINE A SAVED IMAGE CARRIES ("Save image" in the gallery): what this opening is
    //    called and when it became his. ROUND 85: the splash itself never shows it; there is no
    //    held screen any more.
    if (api.card) {
      var line = el("div", "ro-collector");
      line.appendChild(el("span", "ro-collector-no", "No. " + number(o)));
      line.appendChild(el("span", "ro-collector-dot", "·"));
      line.appendChild(el("span", null, title(o)));
      var prov = provenance(o);
      if (prov) {
        line.appendChild(el("span", "ro-collector-dot", "·"));
        line.appendChild(el("span", null, prov));
      }
      root.appendChild(line);
    }

    // ROUND 85: section 5, the light that followed his pointer, is removed.

    // 5. THE PARTICLES: sparks thrown by the landing, or a light for every job.
    if (numbered && (fam.fx === "sparks" || fam.fx === "lights" || o.struck)) {
      var canvas = el("canvas", "ro-fx");
      root.appendChild(canvas);
      live.fx = makeFx(canvas, live);
    }

    // 6. THE SOUND. Its file is made ready a second in (`frame()`), so the notes are on time
    //    and the launch fetched nothing for them. Never in the page nobody sees.
    if (!api.silent && sound.on()) live.soundDue = live.landAt - SOUND_LEAD_MS;

    window.addEventListener("resize", onResize, true);
    // The layout can change with no resize: the line's face arriving after the first frame
    // moves the plinth's edges. Either tells `geometry()` to measure again, once.
    var here = live;
    try {
      if (window.ResizeObserver) {
        live.watch = new ResizeObserver(function () { here.geo = null; });
        live.watch.observe(plinth);
      }
      if (document.fonts && document.fonts.ready) document.fonts.ready.then(function () { here.geo = null; });
    } catch (_e) {
      /* the first measure stands */
    }
    if (!api.native && !api.drawing) live.raf = requestAnimationFrame(frame);
  }

  function onResize() {
    if (live) live.geo = null;
  }

  /// Where an element rests in the window once nothing is moving it: its layout box, read
  /// through its offset parents, which no transform (a stage still rising) changes.
  function restRect(node) {
    var left = 0, top = 0;
    for (var n = node; n; n = n.offsetParent) {
      left += n.offsetLeft;
      top += n.offsetTop;
    }
    return { left: left, top: top, right: left + node.offsetWidth, bottom: top + node.offsetHeight, width: node.offsetWidth, height: node.offsetHeight };
  }

  /// WHERE THE PLINTH AND THE BAR REST, read once and again after a resize: what the particles
  /// are laid out around and thrown from, and where the handover starts.
  function geometry() {
    // A held moment is painted for a picture, not for a frame: it measures afresh, so the
    // picture never stands on a layout the page has since changed (a face arriving).
    if (live.api.held && live.api.held()) live.geo = null;
    if (live.geo) return live.geo;
    if (!live.root.isConnected) return null;
    var geo = { plinth: restRect(live.plinth), bar: live.bar ? restRect(live.bar) : null };
    if (!geo.bar) geo.bar = geo.plinth;
    live.geo = geo;
    return geo;
  }

  /// The state every moving thing is going to, drawn at once: the frame the screen is pinned to
  /// when it leaves early, and the still a saved image is taken from.
  function settle() {
    if (!live) return;
    live.settled = true;
    live.landed = true;
    live.root.classList.add("splash--landed");
    if (live.fx) live.fx.settle();
    paint(live.api.elapsed());
  }

  /// EVERYTHING THIS FILE PAINTS, AS IT STANDS `t` MS AFTER THE FIRST FRAME. `splash.js` calls
  /// it for a held moment (`paintMoment`); the running screen calls it from each frame. All of
  /// it is a function of `t` (ROUND 85: the pointer was the one thing that was not).
  function paint(t) {
    if (!live) return;

    // The number, counting up. It is whole by 3.0 s (the CEO's own change to round 30's v2).
    if (live.count) {
      var c = live.count;
      var u = live.settled || reduceMotion() ? 1 : Math.max(0, Math.min(1, (t - c.from) / (c.to - c.from)));
      var text = Math.round(c.target * (1 - Math.pow(1 - u, 3.2))).toLocaleString("en-US");
      if (text !== c.shown) {
        c.shown = text;
        c.node.textContent = text;
      }
    }
    if (live.fx) live.fx.draw(t);
  }

  function frame() {
    if (!live || (!live.root.isConnected && live.started)) {
      stop();
      return;
    }
    live.started = live.started || live.root.isConnected;
    var t = live.api.elapsed();
    paint(t);
    if (live.soundDue !== null && !live.settled) {
      // Made ready a second in (or at once on a splash joined later than that), played when due.
      if (!live.soundAsked && t >= Math.min(SOUND_LOAD_AT_MS, live.soundDue)) {
        live.soundAsked = true;
        if (!sound.prepare(live.soundGrand)) live.soundDue = null;
      }
      if (live.soundDue !== null && t >= live.soundDue) {
        live.soundDue = null;
        sound.play();
      }
    }
    live.raf = requestAnimationFrame(frame);
  }

  function stop() {
    if (!live) return;
    if (live.raf) cancelAnimationFrame(live.raf);
    window.removeEventListener("resize", onResize, true);
    if (live.watch) live.watch.disconnect();
    sound._el = null;
    live = null;
  }

  /// `splash.js` `tick()` calls this on the frame the bar lands.
  function landed() {
    if (!live || live.landed) return;
    live.landed = true;
    live.root.classList.add("splash--landed");
    if (live.fx) live.fx.land(live.api.elapsed());
  }

  function seekLanding(t) {
    if (!live || live.settled) return null;
    live.landed = t >= live.landAt;
    live.root.classList.toggle("splash--landed", live.landed);
    if (live.fx) live.fx.seek(t);
    return live.landed ? live.landAt : null;
  }
  /// How long before the screen's time is up the bar lands. `splash.js` reads it.
  function landEarly() {
    return LAND_EARLY_MS;
  }
  // ---------------------------------------------------------------------------------------
  // The particles
  // ---------------------------------------------------------------------------------------

  function makeFx(canvas, L) {
    var ctx = canvas.getContext("2d");
    var dpr = Math.min(2, L.api.exportScale || window.devicePixelRatio || 1);
    var W = 0, H = 0;
    var rand = seeded(7400 + number(L.o));
    var rank = L.o.rank || 0;
    var lights = null, lightsFor = "", sparks = [], ring = null, landedAt = null, settled = false;
    var lastSeek = -1;
    var kind = L.fam.fx;

    // One soft gold point, drawn once and stamped.
    var dot = document.createElement("canvas");
    dot.width = dot.height = 64;
    var dc = dot.getContext("2d");
    var gr = dc.createRadialGradient(32, 32, 0, 32, 32, 32);
    gr.addColorStop(0, "rgba(255,240,200,1)");
    gr.addColorStop(0.18, "rgba(232,200,120,0.85)");
    gr.addColorStop(0.5, "rgba(194,163,92,0.22)");
    gr.addColorStop(1, "rgba(194,163,92,0)");
    dc.fillStyle = gr;
    dc.fillRect(0, 0, 64, 64);

    function size() {
      var w = window.innerWidth, h = window.innerHeight;
      if (w === W && h === H) return;
      W = w; H = h;
      canvas.width = Math.round(W * dpr);
      canvas.height = Math.round(H * dpr);
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      lights = null;
    }
    // Where the plinth and the bar rest (`geometry()`): read once, never in a frame.
    function plinthRect() {
      return (geometry() || { plinth: { left: 0, top: 0, right: 0, bottom: 0, width: 0, height: 0 } }).plinth;
    }
    function barRect() {
      return (geometry() || { bar: { left: 0, top: 0, width: 0, height: 0 } }).bar;
    }
    function seedLights() {
      // A light for every job, up to what the ground can hold and still read as a field.
      var target = L.count ? L.count.target : Number(String(figureOf(L.o)).replace(/\D/g, "")) || 1;
      var n = Math.min(900, Math.round(target <= 100 ? target : 100 + 62 * Math.sqrt(target - 100)));
      // Laid out around where the plinth RESTS, not where its rise has it at this moment, so
      // the field is the same whenever it is first drawn (a held moment, or a running frame).
      var p = plinthRect();
      var cx = p.left + p.width / 2, cy = p.top + p.height / 2;
      // A fresh sequence each time, so a resize lays out the same field again.
      rand = seeded(7400 + number(L.o));
      lights = [];
      var guard = 0;
      while (lights.length < n && guard++ < n * 30) {
        // Denser near the plinth, thinning toward the edges: the work gathers around the mark.
        var a = rand() * Math.PI * 2;
        var d = Math.pow(rand(), 0.72);
        var x = cx + Math.cos(a) * d * W * 0.56;
        var y = cy + Math.sin(a) * d * H * 0.62;
        if (x < 6 || x > W - 6 || y < 6 || y > H - 6) continue;
        if (x > p.left - 14 && x < p.right + 14 && y > p.top - 14 && y < p.bottom + 14) continue;
        // Nothing behind a saved image's line at the foot: it has to read on a clean ground.
        if (y > H - 78 && Math.abs(x - cx) < 400) continue;
        lights.push({ x: x, y: y, r: 1.6 + rand() * 3.4 + (rand() < 0.06 ? 4 : 0), a: 0.35 + rand() * 0.6, w: 0.6 + rand() * 1.8, ph: rand() * 6.28, order: 0, d: Math.hypot(x - cx, y - cy) });
      }
      // They come on from the mark outward.
      lights.sort(function (m, q) { return m.d - q.d; });
      lights.forEach(function (l, i) { l.order = (i + rand() * 6) / lights.length; });
    }
    function land(t) {
      landedAt = t;
      if (reduceMotion()) return;
      var b = barRect();
      if (kind === "sparks" || L.o.struck) {
        var n = Math.round(26 + 190 * rank);
        for (var i = 0; i < n; i++) {
          var up = 150 + rand() * (260 + 300 * rank);
          sparks.push({
            x: b.left + rand() * b.width, y: b.top + 1,
            vx: (rand() - 0.5) * 150, vy: -up,
            life: 0.38 + rand() * 0.42, born: t + rand() * 90, len: 0.014 + rand() * 0.02, w: 0.8 + rand() * 1.2
          });
        }
      }
      if (L.o.struck) ring = { at: t };
    }
    function seek(t) {
      if (t < L.landAt || landedAt === null || t < lastSeek) {
        landedAt = null;
        sparks = [];
        ring = null;
        if (t >= L.landAt) {
          rand = seeded(7400 + number(L.o));
          if (kind === "lights") {
            size();
            seedLights();
          }
          land(L.landAt);
        }
      }
      lastSeek = t;
    }
    function draw(t) {
      size();
      ctx.clearRect(0, 0, W, H);
      if (kind === "lights") {
        // The field is laid out around where the plinth rests, so it is laid out again when
        // that changes (the line's face arriving moves the plinth's edges): the same layout is
        // always the same field, whichever moment first drew it.
        var geo = geometry();
        if (!geo) return; // not on the page yet: nothing to lay them around
        var around = [geo.plinth.left, geo.plinth.top, geo.plinth.width, geo.plinth.height, W, H].join(",");
        if (!lights || lightsFor !== around) {
          lightsFor = around;
          seedLights();
        }
        var c = L.count;
        var grown = settled || L.landed ? 1 : c ? Math.max(0, Math.min(1, (t - c.from) / (c.to - c.from))) : 1;
        grown = 1 - Math.pow(1 - grown, 2.4);
        var s = t / 1000;
        var wave = landedAt === null ? -1 : (t - landedAt) / 700; // the landing passes through them
        ctx.globalCompositeOperation = "lighter";
        for (var i = 0; i < lights.length; i++) {
          var l = lights[i];
          if (l.order > grown) continue;
          var on = Math.min(1, (grown - l.order) * 14);
          var tw = settled ? 1 : 0.72 + 0.28 * Math.sin(s * l.w + l.ph);
          var boost = 0;
          if (wave >= 0 && wave < 1.4) {
            var reach = wave * Math.max(W, H) * 0.7;
            boost = Math.max(0, 1 - Math.abs(l.d - reach) / 120) * 0.9;
          }
          ctx.globalAlpha = Math.min(1, l.a * on * tw + boost);
          var r = l.r * (1 + boost * 0.8);
          ctx.drawImage(dot, l.x - r, l.y - r, r * 2, r * 2);
        }
        ctx.globalAlpha = 1;
        ctx.globalCompositeOperation = "source-over";
      }
      if (sparks.length) {
        ctx.globalCompositeOperation = "lighter";
        ctx.lineCap = "round";
        var alive = 0;
        for (var j = 0; j < sparks.length; j++) {
          var p = sparks[j];
          var age = (t - p.born) / 1000;
          if (age < 0) { alive++; continue; }
          if (age > p.life) continue;
          alive++;
          var g = 620;
          var x = p.x + p.vx * age, y = p.y + p.vy * age + 0.5 * g * age * age;
          var vy = p.vy + g * age;
          var k = 1 - age / p.life;
          ctx.strokeStyle = "rgba(" + (236 + 19 * k | 0) + "," + (196 + 40 * k | 0) + "," + (112 + 80 * k | 0) + "," + (0.95 * k).toFixed(3) + ")";
          ctx.lineWidth = p.w * (0.5 + 0.5 * k);
          ctx.beginPath();
          ctx.moveTo(x, y);
          ctx.lineTo(x - p.vx * p.len, y - vy * p.len);
          ctx.stroke();
        }
        ctx.globalCompositeOperation = "source-over";
        if (!alive) sparks = [];
      }
      if (ring) {
        var q = (t - ring.at) / 620;
        if (q >= 1) ring = null;
        else {
          var pr = plinthRect();
          var grow = 6 + 54 * (1 - Math.pow(1 - q, 2.2));
          ctx.strokeStyle = "rgba(232,200,120," + (0.5 * (1 - q)).toFixed(3) + ")";
          ctx.lineWidth = 1.5;
          var rx = pr.left - grow, ry = pr.top - grow, rw = pr.width + grow * 2, rh = pr.height + grow * 2, rad = 10 + grow;
          ctx.beginPath();
          ctx.moveTo(rx + rad, ry);
          ctx.arcTo(rx + rw, ry, rx + rw, ry + rh, rad);
          ctx.arcTo(rx + rw, ry + rh, rx, ry + rh, rad);
          ctx.arcTo(rx, ry + rh, rx, ry, rad);
          ctx.arcTo(rx, ry, rx + rw, ry, rad);
          ctx.stroke();
        }
      }
    }
    return {
      canvas: canvas,
      draw: draw,
      land: land,
      seek: seek,
      settle: function () { settled = true; sparks = []; ring = null; }
    };
  }

  // ---------------------------------------------------------------------------------------
  // The handover into the home screen
  // ---------------------------------------------------------------------------------------

  function movieBar() {
    var splash = window.__RICHOS_LAUNCH__ && window.__RICHOS_LAUNCH__.splash;
    var bar = splash && splash.bar;
    if (!Array.isArray(bar) || bar.length !== 4) return null;
    // The native layer fills the content view and crops equally on opposite edges.
    var scale = Math.max(window.innerWidth / 1400, window.innerHeight / 852);
    return {
      left: (window.innerWidth - 1400 * scale) / 2 + bar[0] * scale,
      top: (window.innerHeight - 852 * scale) / 2 + bar[1] * scale,
      width: bar[2] * scale,
      height: bar[3] * scale
    };
  }

  /// `splash.js` `yieldNow()` calls this as the curtain starts to fade. The landed bar leaves
  /// the curtain, crosses to Talk to Rich and becomes its gold edge, so the cut between the two
  /// screens is a designed one. It rides above both; it takes no input and delays nothing.
  function handover(root) {
    try {
      if (reduceMotion() || !root) return;
      var door = document.getElementById("home-enter");
      // Where the bar rests (it has landed and is still), from the measure already taken: the
      // curtain is not measured again on its way out.
      var geo = live && live.root === root ? geometry() : null;
      if (!geo || !geo.bar || !door) return;
      var a = (live.api.native && movieBar()) || geo.bar;
      var b = door.getBoundingClientRect();
      if (!a.width || !b.width || b.bottom <= 0 || b.top >= window.innerHeight) return;
      var line = el("div", "ro-handover");
      line.style.setProperty("left", a.left + "px");
      line.style.setProperty("top", a.top + a.height / 2 - 1 + "px");
      line.style.setProperty("width", a.width + "px");
      document.body.appendChild(line);
      var dx = b.left - a.left, dy = b.top - (a.top + a.height / 2 - 1), sx = b.width / a.width;
      var anim = line.animate(
        [
          { transform: "translate(0,0) scaleX(1)", opacity: 1 },
          { transform: "translate(" + dx + "px," + dy + "px) scaleX(" + sx + ")", opacity: 1, offset: 0.86 },
          { transform: "translate(" + dx + "px," + dy + "px) scaleX(" + sx + ")", opacity: 0 }
        ],
        { duration: HANDOVER_MS + 120, easing: "cubic-bezier(0.62, 0, 0.18, 1)", fill: "forwards" }
      );
      setTimeout(function () {
        door.classList.add("ro-arrived");
        setTimeout(function () { door.classList.remove("ro-arrived"); }, 900);
      }, HANDOVER_MS - 40);
      var done = function () { if (line.parentNode) line.parentNode.removeChild(line); };
      anim.onfinish = done;
      setTimeout(done, HANDOVER_MS + 400);
    } catch (_e) {
      /* the plain fade is still a whole handover */
    }
  }

  window.RichOpenings = {
    catalog: CATALOG,
    families: FAMILIES,
    DAILY: DAILY,
    /// What is his, as the shell last said it; `update()` takes a newer answer (the gallery
    /// asks when it opens, and after each setting he changes).
    record: function () {
      return record;
    },
    update: function (r) {
      record = normalize(r);
      return record;
    },
    byId: byId,
    number: number,
    title: title,
    earnedOn: earnedOn,
    earned: earnedList,
    current: current,
    provenance: provenance,
    figureOf: figureOf,
    captionOf: captionOf,
    longDate: longDate,
    entryId: entryId,
    entries: entries,
    seal: seal,
    pick: pick,
    dress: dress,
    paint: paint,
    settle: settle,
    landed: landed,
    seekLanding: seekLanding,
    landEarly: landEarly,
    handover: handover,
    stop: stop,
    sound: sound
  };
})();

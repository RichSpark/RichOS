// THE OPENING SCREEN — the renderer, and the only code in this feature.
//
// It runs at the very top of <body>, synchronously, so the composition is part of the
// window's FIRST paint rather than something that arrives after it. Everything below is
// pure DOM construction against `splash.css`'s structure; the values all come from one
// entry in `splash-library.js`.
//
// THE ONE CONSTRAINT ABOVE ALL OTHERS: this never delays launch by a frame. It cannot,
// structurally, and the structure is the argument -
//
//   * it adds no `await` to `main.js`'s boot path. `main.js` calls exactly one function
//     from this file, `RichSplash.yieldNow()`, and calls it at the point its own comment
//     already calls "the app is usable";
//   * it does no I/O, reads no file, and awaits no command before painting. What it needs
//     to know (what kind of start this is, which splash was picked, which openings are his)
//     is already in front of this script, put there by the shell;
//   * and it leaves by itself when its time is up - the app reporting ready, held to the
//     screen's own 3 or 5 seconds, or a hard ceiling one second later - so no failure anywhere
//     else in the shell can leave it up.
//
// ROUND 85 (the CEO, 2026-11-16; richos-hq design/mockups/rounds/round-85): IT PLAYS FROM START
// TO FINISH, WHATEVER THE USER DOES. His words: "We no longer need any interactivity in splash
// screens i.e. each splash screen shall play from start to finish regardless of what the user
// might do with their mouse." So nothing a hand does reaches this screen or changes it:
//
//   * a click, a drag or a scroll lands on the curtain and does nothing (`splash.css`), and
//     nothing under it is pressed;
//   * a key does nothing either (`quiet()` below). Until this round the first click or key sent
//     the screen away and the space key held it (CEO §62); both are gone, and with the hold the
//     settings button and the line it showed. A key with ⌘ or ⌃ keeps what the Mac does with
//     it (quit, reload); the app's own shortcuts wait until the splash has played;
//   * the light that followed the pointer is gone (`openings.js`, `splash.css`).
//
// THE HONEST STATES. If the library is missing, unparseable or
// empty, if every entry in it is malformed, or if anything at all throws while drawing,
// the result is the same and it is deliberate: NO SPLASH, and a completely normal launch.
// Never a half-drawn frame, never a fallback that looks like a broken app. `removeSelf()`
// below is what makes a partial build leave nothing behind.
//
// IT HAS NO OFF SWITCH (CEO, 2026-11-14: "We now also need to remove the settings toggle for
// disabling the splash screen. Because the splash screen is now needed for the illusion of a
// super-fast app launch."). Every fresh start draws it.
//
// ROUND 74 (the CEO's order of 2026-11-14; richos-hq design/mockups/rounds/round-74): the
// openings. Everything that round adds lives in `openings.js`, which loads before this file
// and which this file asks at five marked points (which opening, dress it, the landing, the
// handover, stop). One new path, `replay()`, shows a screen a second time; only the gallery's
// Play calls it. Without `openings.js` this file behaves exactly as it did.
//
// WHAT IS NOT HERE, named rather than hidden: tier 2 (a true line about his record) and
// tier 3 (hallmarks) from `richos-splash-micro-game-2026-08-30.md`. Both wait on CEO
// answers to that document's §8. Their seams are marked in `splash.css` - the slot under
// the rule, and the plinth's inner keyline - and neither needs this file redesigned.

"use strict";

(function () {
  /// The clock the hold is measured on. `performance.now()` is monotonic and immune to a
  /// wall-clock adjustment mid-launch; `Date.now()` is the fallback. Neither is stored and
  /// neither is displayed, so no timezone question arises here - the UTC ruling applies to
  /// the durable stamps in `config.rs`, which are epoch millis and therefore already UTC.
  var now = function () {
    return window.performance && window.performance.now ? window.performance.now() : Date.now();
  };

  // -------------------------------------------------------------------------------------
  // THE CLOCK — ONE READING, WHICH EVERYTHING HERE SHARES
  // -------------------------------------------------------------------------------------
  //
  // Everything that moves on this surface is driven from ONE reading: `clock()`. The bar's fill
  // is `clock() - shownAt`, the landing flare is `clock() - bar.landedAt`, and the hold the
  // ceremony is measured against is `holdMs - (clock() - shownAt)` - the same number, which is
  // the property `SPLASH_SECONDS`' comment already names ("the bar reaches 100% at exactly this
  // many seconds").
  function clock() {
    return heldMoment !== null && shownAt !== null ? shownAt + heldMoment : now();
  }

  // -------------------------------------------------------------------------------------
  // FRAME EXPORT AND THE FINAL GOLD BAR
  // -------------------------------------------------------------------------------------
  //
  // The exporter holds this scene at each frame's timestamp, and macOS plays the resulting
  // asset through its last frame. The page also holds the scene to calculate where the final
  // gold bar meets Home. `holdAt(ms)` freezes the clock, CSS animations and bar together:
  //
  //   * the clock: `clock()` returns `shownAt + heldMoment` while held, so the bar and the hold
  //     read the held moment;
  //   * every animation inside `#splash` (the CSS ones and the bar's fade alike, read off the
  //     document rather than listed, so a stage added later is held without anyone remembering
  //     it): paused and set to the moment;
  //   * the bar: painted at once from the moment (`paintMoment`). A web view nobody sees gets no
  //     animation frames at all (the trials measured none), so a hold never waits for one.
  //
  var heldMoment = null;

  // -------------------------------------------------------------------------------------
  // WHICH LAUNCHES GET A CEREMONY
  //
  // CEO, 2026-08-31: "The splash screen is only for when the user starts the app fresh
  // (after quitting." So a fresh launch draws; a crash-restart and a second window do not,
  // and neither is counted as a start. Waking from sleep is not in the list because he
  // struck the category - the app was never quit, so nothing begins and no code here runs.
  //
  // The verdict arrives as a frozen object the shell injects BEFORE any of this page's own
  // scripts (`main.rs`'s `launch_init_script`, via `WebviewWindowBuilder`). It has to: this
  // file decides whether to draw on its first synchronous line, and a splash that appeared
  // on a crash-restart because the answer arrived one tick late would be the wrong ceremony
  // at the worst possible moment - the app just died in front of him.
  //
  // ABSENT MEANS FRESH, and that is the same rule the switch above already follows: an
  // absent value is an absent opinion, not a decision. Nobody having told us how this page
  // came up covers a webview the shell could not inject into, `index.html` opened straight
  // off disk, and the acceptance harness - and every one of those genuinely IS a fresh
  // start of the thing being looked at. The alternative default would make a shell bug or a
  // plain browser silently delete the feature, which is the failure mode nobody notices.
  var KIND_FRESH = "fresh";

  /// What kind of start this is, or `null` when nobody said.
  function launchKind() {
    try {
      var l = window.__RICHOS_LAUNCH__;
      if (l && typeof l.kind === "string" && l.kind) return l.kind;
    } catch (_e) {
      /* fall through */
    }
    return null;
  }

  // How long the curtain takes to clear once the app is ready. The app underneath is
  // already live, focused and reachable throughout it - this is the fade of an inert
  // layer, not a wait.
  var FADE_MS = 180;

  // The ceiling. If nothing ever reports the app ready - a boot that hangs, a script error
  // in main.js, a command that never returns - the splash still leaves. A doorway that can
  // become a wall is worse than no doorway.
  //
  // IT IS DERIVED FROM THE HOLD AND NOT A SECOND LITERAL. It used to be a flat 4000, which
  // was one second past a 3000ms hold by arithmetic nobody had written down - and would have
  // cut a five-second screen off at four. One second of grace over whatever this launch's
  // hold is; 4000 at the default, unchanged.
  var CEILING_GRACE_MS = 1000;

  // =====================================================================================
  //                          THE ONE NUMBER: THREE SECONDS
  // =====================================================================================
  //
  // CEO, 2026-09-01, verbatim: *"And I said '3 seconds default'. How many times do I have
  // to repeat that? 3 SECONDS. Unless I say otherwise."*
  //
  // It is here, in the open, because the answer to "how long does the splash show for?" has
  // to be findable by looking rather than by reading a state machine. It is the WHOLE life
  // of the ceremony: the plinth lands, the loading bar runs, the bar reaches 100% at exactly
  // this many seconds, and the curtain goes.
  var SPLASH_SECONDS = 3;

  // "Maximum 5 seconds for some." A screen may ask for longer by carrying a `seconds` token
  // and it may not ask for longer than this. It is a ceiling on a per-screen override and it
  // is never the default: an absent `seconds` is three.
  var MAX_SPLASH_SECONDS = 5;
  // =====================================================================================
  //
  // IT DOES NOT DELAY THE LAUNCH, and that rule is unchanged and still binding. The app
  // underneath is live from its first frame - so holding the curtain is not a wait, it is a
  // layer that has not faded yet - and the ceiling still fires on a launch that never reports
  // ready. ROUND 85: his first keystroke or touch no longer sends it away; it plays through.

  /// This launch's hold, in ms - `SPLASH_SECONDS` unless the chosen screen asked for its
  /// own. Set once, in `start()`, from the entry that was actually drawn.
  var holdMs = SPLASH_SECONDS * 1000;

  /// How long THIS screen is on the glass. Absent means the CEO's default; anything past the
  /// ceiling is clamped to it rather than granted, because "up to 5" is a limit.
  function secondsFor(entry) {
    var s = entry.tokens.seconds;
    if (s === null || s === undefined) return SPLASH_SECONDS;
    var n = Number(s);
    if (!isFinite(n) || n <= 0) return SPLASH_SECONDS;
    return n > MAX_SPLASH_SECONDS ? MAX_SPLASH_SECONDS : n;
  }

  /// When the composition went up, for measuring the hold against. `null` until it does.
  var shownAt = null;

  /// How much of the hold is left, in ms. Zero once it is served.
  function holdRemaining() {
    if (shownAt === null) return 0;
    var left = holdMs - (clock() - shownAt);
    return left > 0 ? left : 0;
  }

  // Every token an entry must carry, and what it has to be. An entry missing one is not
  // drawn half-dressed; it is dropped from the pool.
  var STRING_TOKENS = [
    "ground", "atmosphere", "vignette", "grainOpacity",
    "lamp", "lampRadius", "lampStop",
    "surface", "surfaceImage", "plinthRadius", "plinthShadow", "plinthOutline",
    "keylineInset", "keylineRadius", "keylineColor",
    "keylineWidth", "keylineStyle", "keylineImage", "keylineShadow",
    "ink", "signal", "rule", "ruleWidth", "tagline",
    "riseDuration", "riseShift",
    "strike", "strikeDelay", "strikeDuration"
  ];
  var STRIKES = ["none", "fill", "bloom"];
  var GILD_ID = "richos-splash-gild";
  var RELIEF_ID = "richos-splash-relief";

  // A MATERIAL LAYER'S VOCABULARY. Every key an entry may put on one layer of the stack,
  // and the CSS property it lands on. The list is the whole contract: a layer is a flat
  // rectangle over the mat with these paint properties and nothing else, so `materials` can
  // describe suede, bookcloth, slate or moire without the renderer knowing that any of
  // those exist. An unknown key is refused rather than ignored — a typo has to fail loudly,
  // not silently drop the layer that was the point of the entry.
  var LAYER_PROPS = {
    background: "background",
    imageBlend: "background-blend-mode",
    blend: "mix-blend-mode",
    opacity: "opacity",
    inset: "inset",
    height: "height",
    // Added for the loading bar, and available to a material for the same reason every
    // other key here is: they are GEOMETRY, not paint. `width` and `transform` are what put
    // the strike heat on the leading edge of a bar 74px across and half of it past the end.
    width: "width",
    transform: "transform",
    radius: "border-radius",
    border: "border",
    borderImage: "border-image",
    shadow: "box-shadow",
    filter: "filter",
    z: "z-index"
  };
  // `mask` sets two properties, and `relief` sets none directly - it points the layer at
  // the filter this entry described. Both are checked, so they belong in the key list.
  var LAYER_KEYS = ["mask", "relief"];
  var PLACEMENTS = ["inner", "outer"];

  // -------------------------------------------------------------------------------------
  // THE LOADING BAR'S VOCABULARY
  //
  // CEO, 2026-09-01: *"A splash screen is something that has a 'loading' progress bar under
  // the `plinth` element."* So a bar is not optional furniture here - an entry without one
  // is not a splash screen and is dropped from the pool, the same way an entry missing its
  // mat is.
  //
  // HIS TWO BARS ARE DIFFERENT OBJECTS and both are his direction: #1 is the `.rule` at the
  // width of the plinth, striking along its own unstruck ghost; #2 is a leather strap of the
  // same hide with the whole run of needle holes punched from the first frame, sewn live in
  // gold thread. One mechanism has to draw both without knowing that either exists, and this
  // is it: A TRACK CARRYING A STACK OF PAINT LAYERS, SOME OF WHOSE WIDTHS FOLLOW THE
  // PROGRESS. The track's paint, the layers' paint and the rhythm are all the entry's; the
  // progress curve, the stacking order and the geometry are the renderer's.
  var BAR_PROPS = {
    gap: "margin-top",
    height: "height",
    radius: "border-radius",
    background: "background",
    outline: "outline",
    shadow: "box-shadow",
    clip: "overflow"
  };
  var BAR_KEYS = ["enterDelay", "enterDuration"];

  // What a layer of the bar may be, and the whole list:
  //
  //   (absent)   a static layer across the whole track
  //   rhythm     a static layer across the SNAPPED RUN - see `pitch` below
  //   progress   its width IS the progress
  //   lead       rides the leading edge of the progress, and goes out when the bar lands
  //   flare      invisible until the bar lands, then one sweep along its length
  var BAR_ROLES = ["rhythm", "progress", "lead", "flare"];

  // WHEN THE BAR BEGINS, measured from the moment the composition goes up. The approved
  // mockups both use 0.60s and for the same reason - it is where the plinth has landed and
  // the bar has something to sit under. It is a renderer constant rather than a token
  // because it is not variation: both of his screens carry the same number.
  var BAR_START_MS = 600;

  // THE PROGRESS CURVE, and it is deliberately not an ease-out. Mostly linear, with one
  // gentle surge early that fades out and a confident slope at the end. It is monotonic, so
  // it never stalls and never jumps, and it arrives at exactly 100% at exactly the hold -
  // at three seconds and at five alike. The classic "stuck at 95%" feeling IS an ease-out;
  // this is the shape the approved mockups both run, and the shape is structure.
  var BAR_SURGE = 0.12;

  // THE ONE FLARE WHEN IT LANDS. Both approved screens sweep a band of light along the bar
  // from off one end to off the other, over a 260%-wide gradient the ENTRY paints; these
  // three numbers are the sweep's geometry and they are identical in both, which is what
  // makes them structure rather than a value belonging to either screen.
  // ROUND 74 (Vic 3.1, "let the ending happen"): 620, was 950. The bar now lands 0.8 s before
  // the screen's time is up (`RichOpenings.landEarly()`), and the flare has to be over inside
  // that with a still, finished frame left after it: 620 ms of flare, then 180 ms of the
  // settled gold. 620 + 180 = 800.
  var FLARE_MS = 620;
  var FLARE_FROM_PCT = 130;
  var FLARE_TO_PCT = -40;
  var FLARE_PEAK_AT = 0.17;

  // The heat leaving the leading edge once there is nowhere further to go.
  var LEAD_FADE_MS = 550;
  var LEAD_FADE_DELAY_MS = 180;

  // THE MARK. Verbatim geometry from the approved compositions, and NOT in the library
  // because it is not a variation: it is identical in every version of every round, so a
  // copy per entry is a copy that can drift.
  //
  // ITS TWO FILLS ARE UNDER REVIEW, AND THIS IS THE ONE PLACE THEY ARE DECIDED.
  //
  // The paths below carry a `role` of `"ink"` or `"signal"` and nothing else. `svgMark()`
  // turns that into a class, `splash.css` points each class at one custom property, and
  // `build()` sets those two properties — `--splash-ink` and `--splash-signal` — from the
  // entry's `ink` and `signal` tokens. That is the WHOLE path from data to pixel, and it
  // has exactly one junction: the two `setProperty` calls in `build()` marked THE MARK'S
  // TWO FILLS.
  //
  // So if the mark turns out to be monochrome — the CEO questioned the two-tone treatment
  // on 2026-09-01, and his own artwork
  // (`assets/logo-wordmark/RichOS-logo_v3.5_black-and-white.svg`) is two paths with one
  // fill — the change is that junction, not this geometry, not the library, and not both
  // entries: point `--splash-signal` at `t.ink` and every screen goes monochrome at once.
  //
  // THE PATH DATA IS NOT IN QUESTION and is not to be touched. Only the fills.
  var LOGO = {
    viewBox: "0 0 744 744",
    paths: [
    { role: "ink", d: "M517,744h227l-85.2-120.4c-41.7-58.7-83.3-117.5-124.9-176.2c-0.7-0.9-1.3-1.9-2.3-3.4c2.6-1.3,5.1-2.5,7.6-3.7c26.2-12.9,50.3-28.8,70.8-49.7c29.9-30.6,47-67.6,53.7-109.4c8.3-52.1,2.1-102.4-22.8-149.2C601.2,57.3,539.1,13.7,455.2,1.8c-4.1-0.6-8.1-1.2-12.2-1.8C295.3,0,147.7,0,0,0c0,133.5,0,266.7,0,400c12.8-17,24.9-34.6,38.5-51c45.4-55.2,99.8-100.2,159.9-138.6c1.1-0.7,2.2-1.5,3.3-2.2c0.3-0.2,0.4-0.5,1-1.1c-3.1-4.3-6.3-8.8-9.5-13.2c-7.2-10-14.4-20-21.6-30c-3.1-4.3-4.6-9-2.1-14c2.5-4.9,7.1-6,12.2-6c93.8,0,187.6,0,281.5,0c3.5,0,7.2-0.2,9.1,3.6c2,3.8,0.2,7-1.9,10.2c-43.9,66.2-87.8,132.5-131.6,198.7c-0.6,1-1.3,1.9-2,2.9c-7.2,9.8-17.5,10.1-24.9,0.3c-12.3-16.4-24.5-32.8-36.7-49.3c-1.2-1.6-2.4-3.1-3.9-5c-6.3,4.3-12.4,8.3-18.3,12.5c-46.1,32.6-89.8,68-128.1,109.6C62.9,494.7,23.5,573.1,8,663.2c-3.6,20.6-4.9,41.6-7.2,62.5c-0.2,1.8-0.5,3.6-0.8,5.4c0,3.7,0,9.3,0,13c1-1.6,2.3-5.2,2.9-6.9c19.5-54,47.4-103.2,86.8-145.2c59.3-63.2,132.4-100.6,217.6-115.2c6.4-1.1,9.7,0.4,13.6,5.5" },
    { role: "signal", d: "M0,400c12.8-17,24.9-34.6,38.5-51c45.4-55.2,99.8-100.2,159.9-138.6c1.1-0.7,2.2-1.5,3.3-2.2c0.3-0.2,0.4-0.5,1-1.1c-3.1-4.3-6.3-8.8-9.5-13.2c-7.2-10-14.4-20-21.6-30c-3.1-4.3-4.6-9-2.1-14c2.5-4.9,7.1-6,12.2-6c93.8,0,187.6,0,281.5,0c3.5,0,7.2-0.2,9.1,3.6c2,3.8,0.2,7-1.9,10.2c-43.9,66.2-87.8,132.5-131.6,198.7c-0.6,1-1.3,1.9-2,2.9c-7.2,9.8-17.5,10.1-24.9,0.3c-12.3-16.4-24.5-32.8-36.7-49.3c-1.2-1.6-2.4-3.1-3.9-5c-6.3,4.3-12.4,8.3-18.3,12.5c-46.1,32.6-89.8,68-128.1,109.6C62.9,494.7,23.5,573.1,8,663.2c-3.6,20.6-4.9,41.6-7.2,62.5c-0.2,1.8-0.5,3.6-0.8,5.4C0,620.7,0,510.3,0,400z" },
    ],
  };
  var WORDMARK = {
    viewBox: "0 0 3299.1 754.5",
    paths: [
    { role: "ink", d: "M517,744h227l-85.2-120.4c-41.7-58.7-83.3-117.5-124.9-176.2c-0.7-0.9-1.3-1.9-2.3-3.4c2.6-1.3,5.1-2.5,7.6-3.7c26.2-12.9,50.3-28.8,70.8-49.7c29.9-30.6,47-67.6,53.7-109.4c8.3-52.1,2.1-102.4-22.8-149.2C601.2,57.3,539.1,13.7,455.2,1.8c-4.1-0.6-8.1-1.2-12.2-1.8C295.3,0,147.7,0,0,0c0,133.5,0,266.7,0,400c12.8-17,24.9-34.6,38.5-51c45.4-55.2,99.8-100.2,159.9-138.6c1.1-0.7,2.2-1.5,3.3-2.2c0.3-0.2,0.4-0.5,1-1.1c-3.1-4.3-6.3-8.8-9.5-13.2c-7.2-10-14.4-20-21.6-30c-3.1-4.3-4.6-9-2.1-14c2.5-4.9,7.1-6,12.2-6c93.8,0,187.6,0,281.5,0c3.5,0,7.2-0.2,9.1,3.6c2,3.8,0.2,7-1.9,10.2c-43.9,66.2-87.8,132.5-131.6,198.7c-0.6,1-1.3,1.9-2,2.9c-7.2,9.8-17.5,10.1-24.9,0.3c-12.3-16.4-24.5-32.8-36.7-49.3c-1.2-1.6-2.4-3.1-3.9-5c-6.3,4.3-12.4,8.3-18.3,12.5c-46.1,32.6-89.8,68-128.1,109.6C62.9,494.7,23.5,573.1,8,663.2c-3.6,20.6-4.9,41.6-7.2,62.5c-0.2,1.8-0.5,3.6-0.8,5.4c0,3.7,0,9.3,0,13c1-1.6,2.3-5.2,2.9-6.9c19.5-54,47.4-103.2,86.8-145.2c59.3-63.2,132.4-100.6,217.6-115.2c6.4-1.1,9.7,0.4,13.6,5.5" },
    { role: "signal", d: "M0,400c12.8-17,24.9-34.6,38.5-51c45.4-55.2,99.8-100.2,159.9-138.6c1.1-0.7,2.2-1.5,3.3-2.2c0.3-0.2,0.4-0.5,1-1.1c-3.1-4.3-6.3-8.8-9.5-13.2c-7.2-10-14.4-20-21.6-30c-3.1-4.3-4.6-9-2.1-14c2.5-4.9,7.1-6,12.2-6c93.8,0,187.6,0,281.5,0c3.5,0,7.2-0.2,9.1,3.6c2,3.8,0.2,7-1.9,10.2c-43.9,66.2-87.8,132.5-131.6,198.7c-0.6,1-1.3,1.9-2,2.9c-7.2,9.8-17.5,10.1-24.9,0.3c-12.3-16.4-24.5-32.8-36.7-49.3c-1.2-1.6-2.4-3.1-3.9-5c-6.3,4.3-12.4,8.3-18.3,12.5c-46.1,32.6-89.8,68-128.1,109.6C62.9,494.7,23.5,573.1,8,663.2c-3.6,20.6-4.9,41.6-7.2,62.5c-0.2,1.8-0.5,3.6-0.8,5.4C0,620.7,0,510.3,0,400z" },
    { role: "ink", d: "M809.5,58.3H946v124.2H809.5V58.3z M945.1,744H811.4V241.7h133.7V744z" },
    { role: "ink", d: "M1275.5,641.8c55.4,0,88.8-38.2,101.2-89.8l115.6,51.6c-22.9,82.1-101.2,150.9-217.7,150.9c-146.1,0-249.3-106-249.3-261.7c0-154.7,103.1-260.7,249.3-260.7c115.6,0,192,66.8,215.8,148l-113.6,53.5c-12.4-51.6-45.8-89.8-101.2-89.8c-70.7,0-117.5,55.4-117.5,149C1158,587.4,1204.8,641.8,1275.5,641.8z" },
    { role: "ink", d: "M1556.3,58.3H1690v234.9c27.7-32.5,73.5-61.1,137.5-61.1c105.1,0,169,71.6,169,180.5V744h-133.7V450.8c0-56.3-22.9-96.5-80.2-96.5c-46.8,0-92.6,33.4-92.6,99.3V744h-133.7V58.3z" },
    { role: "ink", d: "M2398.6,47.8c188.1,0,323.7,146.1,323.7,353.4s-135.6,353.4-323.7,353.4c-189.1,0-324.7-146.1-324.7-353.4S2209.5,47.8,2398.6,47.8z M2398.6,171c-107.9,0-183.4,89.8-183.4,230.2c0,140.4,75.4,230.2,183.4,230.2c107,0,182.4-89.8,182.4-230.2C2581,260.8,2505.5,171,2398.6,171z" },
    { role: "ink", d: "M2836,525.3c50.6,71.6,124.2,111.7,199.6,111.7c71.6,0,124.2-31.5,124.2-89.8c0-63-64.9-71.6-168.1-94.5c-102.2-23.9-213-56.3-213-192c0-130.8,112.7-213,255-213c118.4,0,211.1,52.5,258.8,121.3l-92.6,90.7c-40.1-56.3-93.6-94.5-170-94.5c-66.9,0-112.7,32.5-112.7,82.1c0,53.5,49.7,64,139.4,84c108.9,23.9,242.6,51.6,242.6,201.5c0,138.5-122.2,221.6-268.4,221.6c-115.6,0-235.9-52.5-291.3-135.6L2836,525.3z" },
    ],
  };

  // THE LINE UNDER THE RULE. The CEO's words, and his size.
  //
  // Both are here rather than in `splash.css` for the same reason the text always has been:
  // they are not variation. Every approved screen carries this line at this size, so a copy
  // per entry is a copy that can drift, and a rule in the stylesheet would be a second place
  // to look. The size is his instruction of 2026-09-01 and it is also §15's floor argued the
  // other way round: 18px is above the 16px minimum for text meant to be read, and this line
  // is the only text on the surface.
  //
  // The tracking is the approved mockups' 0.10em and not the 0.15em this surface shipped at
  // 12px: at 18px the wider setting runs the line past the plinth it sits under.
  var LINE = "The Operating System for the AI-Enabled CEO";
  var LINE_SIZE = "18px";
  var LINE_TRACKING = "0.10em";

  // -------------------------------------------------------------------------------------
  // The library
  // -------------------------------------------------------------------------------------

  /// The material stack, checked key by key. A layer that carries a key this renderer does
  /// not know is a layer whose author expected something to happen that will not — so the
  /// ENTRY is dropped, not the key. Silently ignoring it is how a suede mat ships as a
  /// plain one.
  function validMaterials(layers) {
    if (!Array.isArray(layers)) return false;
    for (var i = 0; i < layers.length; i++) {
      var l = layers[i];
      if (!l || typeof l !== "object" || Array.isArray(l)) return false;
      if (typeof l.background !== "string" || !l.background) return false;
      for (var k in l) {
        if (!l.hasOwnProperty(k)) continue;
        if (k === "relief") {
          if (typeof l.relief !== "boolean") return false;
          continue;
        }
        if (!LAYER_PROPS.hasOwnProperty(k) && LAYER_KEYS.indexOf(k) < 0) return false;
        if (typeof l[k] !== "string" || !l[k]) return false;
      }
    }
    return true;
  }

  /// The relief spec, which is an SVG filter written as values rather than as markup. The
  /// SHAPE of the filter is fixed below in `relief()` — a noise field clipped inside the
  /// mark, and any number of shadow or light bands laid inside or outside it. What each
  /// band is made of is the entry's to say. This is the same division the gilding gradient
  /// already lives under: the structure is the renderer's, the values are the library's.
  function validRelief(spec) {
    if (spec === null) return true;
    if (typeof spec !== "object" || Array.isArray(spec)) return false;
    if (spec.target !== "mark" && spec.target !== "signal") return false;
    if (typeof spec.region !== "string" || spec.region.trim().split(/\s+/).length !== 4) return false;
    if (spec.noise !== null) {
      var n = spec.noise;
      if (!n || typeof n !== "object") return false;
      var keys = ["type", "baseFrequency", "octaves", "seed"];
      for (var i = 0; i < keys.length; i++) {
        if (typeof n[keys[i]] !== "string" || !n[keys[i]]) return false;
      }
      if (typeof spec.matrix !== "string" || !spec.matrix) return false;
    }
    if (!Array.isArray(spec.bands)) return false;
    for (var j = 0; j < spec.bands.length; j++) {
      var b = spec.bands[j];
      if (!b || typeof b !== "object") return false;
      if (PLACEMENTS.indexOf(b.placement) < 0) return false;
      var bk = ["color", "opacity", "dx", "dy", "blur"];
      for (var m = 0; m < bk.length; m++) {
        if (typeof b[bk[m]] !== "string" || !b[bk[m]]) return false;
      }
    }
    return true;
  }

  /// One layer of the bar. The same vocabulary a material layer takes - because a layer is
  /// a layer - plus `role`, and minus `relief`, which is the mark's business and not a
  /// bar's. An unknown key is refused rather than ignored, for the reason `validMaterials`
  /// gives: a typo has to fail loudly, not silently drop the layer that was the point.
  function validBarLayers(layers) {
    if (!Array.isArray(layers) || !layers.length) return false;
    for (var i = 0; i < layers.length; i++) {
      var l = layers[i];
      if (!l || typeof l !== "object" || Array.isArray(l)) return false;
      if (typeof l.background !== "string" || !l.background) return false;
      for (var k in l) {
        if (!l.hasOwnProperty(k)) continue;
        if (k === "role") {
          if (BAR_ROLES.indexOf(l.role) < 0) return false;
          continue;
        }
        if (!LAYER_PROPS.hasOwnProperty(k) && k !== "mask") return false;
        if (typeof l[k] !== "string" || !l[k]) return false;
      }
    }
    return true;
  }

  /// The bar. A splash screen has one, so this is not nullable - an entry whose bar is
  /// missing, malformed, or has no layer that actually progresses is not drawn.
  function validBar(b) {
    if (!b || typeof b !== "object" || Array.isArray(b)) return false;
    for (var p in BAR_PROPS) {
      if (!BAR_PROPS.hasOwnProperty(p)) continue;
      if (typeof b[p] !== "string" || !b[p]) return false;
    }
    for (var i = 0; i < BAR_KEYS.length; i++) {
      if (typeof b[BAR_KEYS[i]] !== "string" || !b[BAR_KEYS[i]]) return false;
    }
    // `pitch` is the one genuinely optional thing about a bar: #2 has a rhythm at the plinth
    // border's 10.5px and #1 has none at all.
    if (b.pitch !== null && (typeof b.pitch !== "string" || !b.pitch)) return false;
    if (typeof b.pitchInset !== "string" || !b.pitchInset) return false;
    if (!validBarLayers(b.layers)) return false;
    for (var j = 0; j < b.layers.length; j++) {
      if (b.layers[j].role === "progress") return true;
    }
    return false;
  }

  function valid(entry) {
    if (!entry || typeof entry !== "object") return false;
    if (typeof entry.id !== "string" || !entry.id) return false;
    var t = entry.tokens;
    if (!t || typeof t !== "object") return false;
    for (var i = 0; i < STRING_TOKENS.length; i++) {
      if (typeof t[STRING_TOKENS[i]] !== "string" || !t[STRING_TOKENS[i]]) return false;
    }
    if (STRIKES.indexOf(t.strike) < 0) return false;
    if (!Array.isArray(t.riseDelays) || t.riseDelays.length !== 4) return false;
    for (var j = 0; j < 4; j++) {
      if (typeof t.riseDelays[j] !== "string" || !t.riseDelays[j]) return false;
    }
    if (t.gild !== null && !(Array.isArray(t.gild) && t.gild.length === 3)) return false;
    if (t.sheen !== null && typeof t.sheen !== "string") return false;
    if (t.markFilter !== null && typeof t.markFilter !== "string") return false;
    if (t.signalFilter !== null && typeof t.signalFilter !== "string") return false;
    if (!validMaterials(t.materials)) return false;
    if (!validRelief(t.relief)) return false;
    if (!validBar(t.bar)) return false;
    if (t.seconds !== null && t.seconds !== undefined) {
      var secs = Number(t.seconds);
      if (typeof t.seconds !== "string" || !isFinite(secs) || secs <= 0) return false;
    }
    if (t.strike === "fill") {
      // A colour strike on a gradient fill has no colour to animate: the gold would simply
      // never arrive. Refuse the entry rather than draw a mark that stays unlit.
      if (t.gild !== null) return false;
      if (typeof t.strikeFrom !== "string" || typeof t.strikePeak !== "string") return false;
    }
    return true;
  }

  function pool() {
    var lib = window.RichSplashLibrary;
    if (!lib || !Array.isArray(lib.variations)) return [];
    var out = [];
    for (var i = 0; i < lib.variations.length; i++) {
      if (valid(lib.variations[i])) out.push(lib.variations[i]);
    }
    // ROUND 74: the openings, after the library's own and in the catalog's order (the list the
    // shell builds). Only where the library itself is usable: with no entry of its own there is
    // no splash, as before.
    if (out.length && RO) {
      var more = RO.entries();
      for (var j = 0; j < more.length; j++) {
        if (valid(more[j])) out.push(more[j]);
      }
    }
    return out;
  }

  // -------------------------------------------------------------------------------------
  // WHICH SCREEN HE SEES, AND IT IS NOT A DRAW
  //
  // CEO, 2026-09-01, verbatim: *"USE THE APPROVED SPLASH SCREEN #1 to always
  // deterministically show as SPLASH SCREEN #1 and splash screen #2 to always show as splash
  // screen for the SECOND APP START in RichOS v1. From the third app start onwards: Only
  // splash screen #1. For NOW == app version 1."*
  //
  //     start 1        -> #1
  //     start 2        -> #2
  //     start 3 and on -> #1
  //
  // THIS REPLACED A RANDOM DRAW. Until this commit the surface picked uniformly out of the
  // library with a no-immediate-repeat guard, which is the mechanism his LATER framing asks
  // for - *"eventually, there will be many splash screens added to the array where the splash
  // screen will be randomly picked for the user (and sometimes semi-randomly or
  // deterministically picked for a user based on certain criteria)"* - and is not what he
  // asked for in v1. `KEY_LAST`, the no-repeat guard's storage, went with it: a deterministic
  // rule has nothing to remember.
  //
  // WHERE THE RULES LIVE NOW: in the shell, `richos-core`'s `launch::pick_splash`, applied when
  // the app quits (Sage's plan 2026-11-14, section 1: the shell needs the answer before this page
  // exists, so the opening can be drawn ahead and played at the click). The shell puts its pick in
  // front of this page (`window.__RICHOS_LAUNCH__.splash`) and `start()` draws it. `choose()` is
  // this page's own fallback where nothing was injected or the id is unknown here (a plain
  // browser, the acceptance suite, the first start after an update to a build that dropped the
  // picked entry), and it gives the same answer as `pick_splash` for the same start; a later rule
  // is a change to `pick_splash`. The pool is an ARRAY and the rule reads its length, so a third
  // screen is a third object in the library.
  //
  // A LIBRARY OF ONE gets that one every time, which is the only honest answer: a rule that
  // says "the second start shows the second screen" cannot be applied where there is no
  // second screen, and refusing to draw would be a worse answer than drawing #1 twice.

  /// WHICH START THIS IS, 1-based, or `null` when nobody could say.
  ///
  /// The shell injects it beside `kind`, read out of `launches.json` - the durable launch
  /// record that already exists (`crates/richos-core/src/launch.rs`), never a second counter
  /// kept here. Three real cases produce `null` and all three mean the same thing: a webview
  /// the shell could not inject into, `index.html` opened straight off disk, and a launch
  /// record that would not parse. `choose()` reads all three as the first start, which shows
  /// #1 - the answer that is never wrong to give.
  function launchOrdinal() {
    try {
      var l = window.__RICHOS_LAUNCH__;
      if (l && typeof l.ordinal === "number" && isFinite(l.ordinal) && l.ordinal > 0) {
        return Math.floor(l.ordinal);
      }
    } catch (_e) {
      /* fall through */
    }
    return null;
  }

  function choose(candidates, ordinal) {
    // ROUND 74: the user's own pick first, as `pick_splash` reads it. Null means he has none,
    // and the rule below stands.
    if (RO) {
      var picked = RO.pick(candidates);
      if (picked) return picked;
    }
    if (candidates.length < 2) return candidates[0];
    return ordinal === 2 ? candidates[1] : candidates[0];
  }

  // -------------------------------------------------------------------------------------
  // Drawing
  // -------------------------------------------------------------------------------------

  var SVG_NS = "http://www.w3.org/2000/svg";

  function svgMark(mark, className, gilded, reliefSpec) {
    var svg = document.createElementNS(SVG_NS, "svg");
    svg.setAttribute("viewBox", mark.viewBox);
    svg.setAttribute("class", className);
    svg.setAttribute("aria-hidden", "true");
    svg.setAttribute("focusable", "false");
    if (gilded || reliefSpec) {
      var defs = document.createElementNS(SVG_NS, "defs");
      if (gilded) defs.appendChild(gilding(gilded));
      if (reliefSpec) defs.appendChild(relief(reliefSpec));
      svg.appendChild(defs);
    }
    for (var i = 0; i < mark.paths.length; i++) {
      var p = document.createElementNS(SVG_NS, "path");
      p.setAttribute("d", mark.paths[i].d);
      p.setAttribute("class", mark.paths[i].role === "signal" ? "splash-signal" : "splash-ink");
      svg.appendChild(p);
    }
    return svg;
  }

  /// The gradient v2 and v6 give the gold, built from the entry's own three stops. Defined
  /// once, inside the first mark; `url(#id)` resolves document-wide, which is exactly how
  /// the studies do it.
  function gilding(stops) {
    var grad = document.createElementNS(SVG_NS, "linearGradient");
    grad.setAttribute("id", GILD_ID);
    grad.setAttribute("x1", "0");
    grad.setAttribute("y1", "0");
    grad.setAttribute("x2", "0.18");
    grad.setAttribute("y2", "1");
    var offsets = ["0", "0.42", "1"];
    for (var i = 0; i < stops.length; i++) {
      var stop = document.createElementNS(SVG_NS, "stop");
      stop.setAttribute("offset", offsets[i]);
      stop.setAttribute("stop-color", stops[i]);
      grad.appendChild(stop);
    }
    return grad;
  }

  /// THE RELIEF FILTER — the mark's material, built from the entry's own numbers.
  ///
  /// Its SHAPE is fixed here and its VALUES all arrive in `spec`, which is the same
  /// division `gilding()` above already lives under. Two ingredients, either of which may
  /// be absent:
  ///
  ///   * a NOISE FIELD, coloured through a matrix and clipped to the mark's own alpha —
  ///     what makes maki-e gold dust sit in the lacquered arrow and bullion thread run
  ///     across the mark in strands;
  ///   * any number of BANDS, each a flood of one colour clipped against a blurred, offset
  ///     copy of the mark. `inner` leaves the band inside the glyph, which is a letter
  ///     pressed INTO the material; `outer` leaves it outside, which is one sitting proud
  ///     of it. There is no CSS filter function for the inner case, which is why this is
  ///     an SVG filter and not a `drop-shadow()` list.
  ///
  /// The merge order is the one thing that is not negotiable and so is not data: outer
  /// bands under the mark, the mark, then the noise and the inner bands over it. Any other
  /// order draws a shadow on top of the thing casting it.
  function relief(spec) {
    var f = document.createElementNS(SVG_NS, "filter");
    f.setAttribute("id", RELIEF_ID);
    var region = spec.region.trim().split(/\s+/);
    f.setAttribute("x", region[0]);
    f.setAttribute("y", region[1]);
    f.setAttribute("width", region[2]);
    f.setAttribute("height", region[3]);

    function prim(tag, attrs) {
      var n = document.createElementNS(SVG_NS, tag);
      for (var k in attrs) {
        if (attrs.hasOwnProperty(k)) n.setAttribute(k, attrs[k]);
      }
      f.appendChild(n);
      return n;
    }

    var under = [];
    var over = [];

    if (spec.noise) {
      prim("feTurbulence", {
        type: spec.noise.type,
        baseFrequency: spec.noise.baseFrequency,
        numOctaves: spec.noise.octaves,
        seed: spec.noise.seed,
        result: "n"
      });
      prim("feColorMatrix", { in: "n", type: "matrix", values: spec.matrix, result: "nc" });
      prim("feComposite", { in: "nc", in2: "SourceAlpha", operator: "in", result: "grain" });
      over.push("grain");
    }

    for (var i = 0; i < spec.bands.length; i++) {
      var b = spec.bands[i];
      var id = "band" + i;
      prim("feFlood", { "flood-color": b.color, "flood-opacity": b.opacity, result: id + "f" });
      prim("feOffset", { in: "SourceAlpha", dx: b.dx, dy: b.dy, result: id + "o" });
      prim("feGaussianBlur", { in: id + "o", stdDeviation: b.blur, result: id + "g" });
      if (b.placement === "outer") {
        prim("feComposite", { in: id + "f", in2: id + "g", operator: "in", result: id + "m" });
        prim("feComposite", { in: id + "m", in2: "SourceAlpha", operator: "out", result: id });
        under.push(id);
      } else {
        prim("feComposite", { in: id + "f", in2: "SourceAlpha", operator: "in", result: id + "m" });
        prim("feComposite", { in: id + "m", in2: id + "g", operator: "out", result: id });
        over.push(id);
      }
    }

    var merge = prim("feMerge", {});
    var order = under.concat(["SourceGraphic"], over);
    for (var j = 0; j < order.length; j++) {
      var node = document.createElementNS(SVG_NS, "feMergeNode");
      node.setAttribute("in", order[j]);
      merge.appendChild(node);
    }
    return f;
  }

  /// One layer of the material stack. Every property it paints with comes off the entry;
  /// this function knows only where a layer sits and which CSS property each key drives.
  function materialLayer(spec) {
    var el = div("splash-material");
    for (var k in spec) {
      if (!spec.hasOwnProperty(k)) continue;
      if (k === "relief") {
        if (spec.relief) el.style.setProperty("filter", "url(#" + RELIEF_ID + ")");
        continue;
      }
      if (k === "mask") {
        // WebKit still wants the prefixed property; setting both is what makes the same
        // entry look the same under the engine Tauri ships and under the one it does not.
        el.style.setProperty("-webkit-mask-image", spec.mask);
        el.style.setProperty("mask-image", spec.mask);
        continue;
      }
      el.style.setProperty(LAYER_PROPS[k], spec[k]);
    }
    return el;
  }

  function div(className) {
    var d = document.createElement("div");
    d.className = className;
    return d;
  }

  // -------------------------------------------------------------------------------------
  // THE LOADING BAR
  //
  // It is built here, it runs ONCE, and there is no path in this file that starts it again.
  // CEO, 2026-09-01, verbatim: *"The animation is only supposed to happen ONCE. NO LOOPING."*
  // The approved mockups carry an `AUTO_REPLAY` flag for judging, set to `false`; nothing
  // like it exists here at all. `tick()` below returns without asking for another frame the
  // moment the landing flare is over, and there is no `visibilitychange`, `focus` or
  // `pageshow` listener anywhere in this file that could wake it.
  // -------------------------------------------------------------------------------------

  /// Everything the running bar needs, or `null` before it is built. Held in one object so
  /// `settleBar()` and `removeSelf()` can put it down in one move.
  var bar = null;
  var raf = null;

  function reduceMotion() {
    try {
      return !!(window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches);
    } catch (_e) {
      return false;
    }
  }

  /// A CSS time, in ms. `0.62s` and `620ms` both, because a token may be written either way
  /// and a bar that silently starts at time zero is worse than one that refuses to build.
  function millis(v) {
    var n = parseFloat(v);
    if (!isFinite(n)) return 0;
    return /ms\s*$/.test(v) ? n : n * 1000;
  }

  /// One layer of the bar. Identical in shape to `materialLayer` and deliberately separate:
  /// a material layer may point at the mark's relief filter and a bar layer may not, and
  /// `role` is meaningless on a mat.
  function barLayer(spec) {
    var el = div("splash-bar-layer");
    // The layer says what it is. Not a class - `tests/splash.js` check 6 inventories class
    // names to prove the palette study was left behind, and five more of them would be five
    // more things to allow. A data attribute is the layer describing itself to a reader.
    if (spec.role) el.dataset.role = spec.role;
    el.style.setProperty("position", "absolute");
    el.style.setProperty("inset", "0");
    for (var k in spec) {
      if (!spec.hasOwnProperty(k)) continue;
      if (k === "role") continue;
      if (k === "mask") {
        el.style.setProperty("-webkit-mask-image", spec.mask);
        el.style.setProperty("mask-image", spec.mask);
        continue;
      }
      el.style.setProperty(LAYER_PROPS[k], spec[k]);
    }
    return el;
  }

  /// The track, and the stack on it, in the entry's own order.
  ///
  /// EVERY LAYER IS A CHILD OF ONE CONTAINER, on purpose. An earlier shape put the static
  /// layers on the track and the progressing ones in a nested run box, and that quietly
  /// reversed the paint order for #2 - the stitch channel is declared BEFORE the holes and
  /// would have painted over them. Positioned siblings stack in document order, so one
  /// container is the only arrangement in which the entry's order is the order on screen.
  function buildBar(b) {
    var root = div("splash-bar");
    var s = root.style;
    for (var p in BAR_PROPS) {
      if (BAR_PROPS.hasOwnProperty(p)) s.setProperty(BAR_PROPS[p], b[p]);
    }
    s.setProperty("position", "relative");
    s.setProperty("outline-offset", "-1px");
    s.setProperty("flex", "0 0 auto");

    var fills = [];
    var leads = [];
    var flares = [];
    var rhythms = [];
    for (var i = 0; i < b.layers.length; i++) {
      var spec = b.layers[i];
      var el = barLayer(spec);
      if (spec.role === "progress") {
        el.style.setProperty("width", "0%");
        fills.push(el);
        root.appendChild(el);
      } else if (spec.role === "lead") {
        // The lead rides in its own progress-driven wrapper rather than inside a named fill,
        // so it does not depend on which layer happens to be declared last.
        var wrap = div("splash-bar-lead");
        wrap.dataset.role = "lead";
        wrap.style.setProperty("position", "absolute");
        wrap.style.setProperty("top", "0");
        wrap.style.setProperty("bottom", "0");
        wrap.style.setProperty("left", "0");
        wrap.style.setProperty("width", "0%");
        wrap.appendChild(el);
        leads.push(wrap);
        root.appendChild(wrap);
      } else if (spec.role === "flare") {
        el.style.setProperty("opacity", "0");
        flares.push({ el: el, peak: parseFloat(spec.opacity || "1") });
        root.appendChild(el);
      } else {
        if (spec.role === "rhythm") rhythms.push(el);
        root.appendChild(el);
      }
    }

    bar = {
      root: root,
      fills: fills,
      leads: leads,
      flares: flares,
      rhythms: rhythms,
      pitch: b.pitch ? parseFloat(b.pitch) : 0,
      inset: parseFloat(b.pitchInset) || 0,
      landed: false,
      landedAt: 0,
      /// The track's own fade-in, kept rather than fired and forgotten. `null` under reduced
      /// motion, where it is never started.
      enter: null
    };

    // The empty track fades in once the plinth has landed, exactly as the mockups do it.
    // §18's stance is unchanged: no motion for anyone who has asked for none - but the bar
    // itself still moves under reduced motion, because a loading bar that does not move is
    // a broken loading bar, and that is the mockups' own call.
    if (!reduceMotion() && root.animate) {
      bar.enter = root.animate([{ opacity: 0 }, { opacity: 1 }], {
        duration: millis(b.enterDuration),
        delay: millis(b.enterDelay),
        easing: "ease",
        fill: "both"
      });
    }
    return root;
  }

  /// Where the bar's rhythm-snapped run sits, in px, or `null` when it has no rhythm.
  ///
  /// #2's needle holes are punched at the plinth border's 10.5px pitch, and the strap is
  /// whatever width the plinth is - so the run has to be a WHOLE number of pitches, centered,
  /// or the last hole is sliced in half by the strap's own rounded end. #1 has no rhythm and
  /// takes the whole track, which is what a null means.
  function runBox() {
    if (!bar.pitch) return null;
    var full = bar.root.getBoundingClientRect().width;
    var n = Math.floor((full - bar.inset * 2) / bar.pitch);
    if (n < 1) n = 1;
    var width = n * bar.pitch;
    return { left: (full - width) / 2, width: width };
  }

  /// Paint the bar at `p`, from 0 to 1. Reads the layout once and then only writes, so a
  /// frame never interleaves a measure with a mutation.
  function paintBar(p) {
    var box = runBox();
    var i;
    if (box) {
      for (i = 0; i < bar.rhythms.length; i++) {
        bar.rhythms[i].style.setProperty("left", box.left + "px");
        bar.rhythms[i].style.setProperty("right", "auto");
        bar.rhythms[i].style.setProperty("width", box.width + "px");
      }
    }
    var lead = box ? box.left + "px" : "0px";
    var wide = box ? (p * box.width).toFixed(2) + "px" : (p * 100).toFixed(3) + "%";
    for (i = 0; i < bar.fills.length; i++) {
      bar.fills[i].style.setProperty("left", lead);
      bar.fills[i].style.setProperty("right", "auto");
      bar.fills[i].style.setProperty("width", wide);
    }
    for (i = 0; i < bar.leads.length; i++) {
      bar.leads[i].style.setProperty("left", lead);
      bar.leads[i].style.setProperty("width", wide);
    }
  }

  /// The sweep of light along the bar, once, when it lands. `q` runs 0 to 1 over `FLARE_MS`.
  function paintFlare(q) {
    var pos = FLARE_FROM_PCT + (FLARE_TO_PCT - FLARE_FROM_PCT) * q;
    var o = q < FLARE_PEAK_AT ? q / FLARE_PEAK_AT : (1 - q) / (1 - FLARE_PEAK_AT);
    if (o < 0) o = 0;
    for (var i = 0; i < bar.flares.length; i++) {
      bar.flares[i].el.style.setProperty("background-position-x", pos.toFixed(2) + "%");
      bar.flares[i].el.style.setProperty("opacity", (o * bar.flares[i].peak).toFixed(3));
    }
  }

  function hideFlares() {
    for (var i = 0; i < bar.flares.length; i++) bar.flares[i].el.style.setProperty("opacity", "0");
  }

  /// The heat goes out of the leading edge, because there is nowhere further for it to go.
  function retireLeads(animated) {
    for (var i = 0; i < bar.leads.length; i++) {
      if (animated) {
        bar.leads[i].style.setProperty(
          "transition",
          "opacity " + LEAD_FADE_MS + "ms ease " + LEAD_FADE_DELAY_MS + "ms"
        );
      }
      bar.leads[i].style.setProperty("opacity", "0");
    }
  }

  /// Put the frame loop down, and SAY SO. Every exit from `tick()` that does not schedule
  /// another frame goes through here, so `state.barStopped` cannot disagree with reality.
  function stopTicking() {
    raf = null;
    state.barStopped = true;
  }

  /// How far the bar has run at `t` ms after the first frame, 0 to 1, and its paint on THE
  /// PROGRESS CURVE. One reading for the running bar and the held one.
  function barSpan() {
    // ROUND 74: the bar lands early, so its ending happens inside the screen's own time.
    var span = holdMs - BAR_START_MS - (RO ? RO.landEarly() : 0);
    return span < 350 ? 350 : span;
  }
  function barRun(t) {
    var u = (t - BAR_START_MS) / barSpan();
    return u < 0 ? 0 : u > 1 ? 1 : u;
  }
  function barCurve(u) {
    return u + BAR_SURGE * Math.sin(2 * Math.PI * u) * (1 - u);
  }

  /// The bar exactly as it stands `t` ms after the first frame, painted now and without a frame
  /// (a held splash). It changes no record of the bar's one pass: holding is looking, not running.
  function paintMoment(t) {
    // ROUND 74: what the openings add stands at the same moment (the number, the lights).
    if (RO) RO.paint(t);
    if (!bar) return;
    var u = barRun(t);
    paintBar(barCurve(u));
    var i;
    if (u < 1) {
      for (i = 0; i < bar.leads.length; i++) {
        bar.leads[i].style.removeProperty("transition");
        bar.leads[i].style.removeProperty("opacity");
      }
      hideFlares();
      return;
    }
    retireLeads(false);
    var q = (t - BAR_START_MS - barSpan()) / FLARE_MS;
    if (q >= 0 && q < 1 && !reduceMotion()) paintFlare(q);
    else hideFlares();
  }

  /// ONE PASS, AND THEN IT STOPS. The only exits that do not schedule another frame are the
  /// two that call `stopTicking()`, and there is no other caller of `tick`.
  function tick() {
    if (!bar) return;
    // A held splash is painted by its hold, never by a frame.
    if (heldMoment !== null) {
      raf = null;
      return;
    }
    var u = barRun(clock() - shownAt);
    paintBar(barCurve(u));

    if (u >= 1 && !bar.landed) {
      bar.landed = true;
      bar.landedAt = clock();
      state.barPasses++;
      if (RO) RO.landed(); // ROUND 74: the strike, the sparks, the sound
      retireLeads(!reduceMotion());
      if (reduceMotion()) {
        stopTicking();
        return;
      }
    }
    if (bar.landed) {
      var q = (clock() - bar.landedAt) / FLARE_MS;
      if (q >= 1) {
        hideFlares();
        stopTicking();
        return;
      }
      paintFlare(q);
    }
    raf = requestAnimationFrame(tick);
  }

  /// The bar's half of `splash--settled`: the ceremony is cut, and the bar is pinned FULL.
  ///
  /// Full and not frozen-at-40%, deliberately. The curtain only leaves when the app is up or
  /// when he touches something, and in both cases loading is over - a bar left standing at a
  /// fraction on the way out would be the one frame on this surface that says something
  /// untrue. The flare and the heat are performance and are cut, exactly as the strike is.
  function settleBar() {
    if (!bar) return;
    if (raf !== null) cancelAnimationFrame(raf);
    stopTicking();
    if (!bar.landed) state.barPasses++;
    paintBar(1);
    retireLeads(false);
    hideFlares();
    bar.landed = true;
  }


  function build(entry, silent) {
    var t = entry.tokens;
    var root = div("splash");
    root.id = "splash";
    root.setAttribute("aria-hidden", "true");
    root.dataset.variation = entry.id;

    var s = root.style;
    s.setProperty("--splash-ground", t.ground);
    s.setProperty("--splash-atmosphere", t.atmosphere);
    s.setProperty("--splash-vignette", t.vignette);
    s.setProperty("--splash-grain-opacity", t.grainOpacity);
    s.setProperty("--splash-lamp", t.lamp);
    s.setProperty("--splash-lamp-radius", t.lampRadius);
    s.setProperty("--splash-lamp-stop", t.lampStop);
    s.setProperty("--splash-surface", t.surface);
    s.setProperty("--splash-surface-image", t.surfaceImage);
    s.setProperty("--splash-plinth-radius", t.plinthRadius);
    s.setProperty("--splash-plinth-shadow", t.plinthShadow);
    s.setProperty("--splash-plinth-outline", t.plinthOutline);
    s.setProperty("--splash-keyline-inset", t.keylineInset);
    s.setProperty("--splash-keyline-radius", t.keylineRadius);
    s.setProperty("--splash-keyline-color", t.keylineColor);
    s.setProperty("--splash-keyline-width", t.keylineWidth);
    s.setProperty("--splash-keyline-style", t.keylineStyle);
    s.setProperty("--splash-keyline-image", t.keylineImage);
    s.setProperty("--splash-keyline-shadow", t.keylineShadow);
    // THE MARK'S TWO FILLS — the junction named in the LOGO comment above, and the only
    // place either one is decided. One line each, both screens.
    s.setProperty("--splash-ink", t.ink);
    // Gilded gold is a paint server with the flat colour as its fallback, exactly as the
    // studies write it (`fill: url(#gild) <the flat gold>` — the hex itself deliberately
    // not repeated here, because `tests/splash.js` check 2 greps this whole file for one
    // and a value quoted in a comment is the first step toward a value used in code). One
    // property, so the settled state and the bloom strike need no second rule to know
    // which kind of gold they are pinning.
    s.setProperty("--splash-signal", t.gild ? "url(#" + GILD_ID + ") " + t.signal : t.signal);
    // (End of THE MARK'S TWO FILLS.)
    s.setProperty("--splash-rule", t.rule);
    s.setProperty("--splash-rule-width", t.ruleWidth);
    s.setProperty("--splash-tagline", t.tagline);
    s.setProperty("--splash-sheen", t.sheen || "none");
    s.setProperty("--splash-rise-duration", t.riseDuration);
    s.setProperty("--splash-rise-shift", t.riseShift);
    s.setProperty("--splash-rise-d1", t.riseDelays[0]);
    s.setProperty("--splash-rise-d2", t.riseDelays[1]);
    s.setProperty("--splash-rise-d3", t.riseDelays[2]);
    s.setProperty("--splash-rise-d4", t.riseDelays[3]);
    s.setProperty("--splash-strike-delay", t.strikeDelay);
    s.setProperty("--splash-strike-duration", t.strikeDuration);
    s.setProperty("--splash-strike-from", t.strikeFrom || t.signal);
    s.setProperty("--splash-strike-peak", t.strikePeak || t.signal);
    s.setProperty("--splash-fade", FADE_MS + "ms");
    // The mark's relief, resolved once. A `relief` spec aimed at the whole mark reaches the
    // gold too — the studies write it as `.logo path, .wordmark path`, which is every path
    // there is — while one aimed at `signal` leaves the ink flat. A plain CSS filter the
    // entry wrote out wins over both, because an entry that says exactly what it wants is
    // not asking to be interpreted.
    var reliefUrl = t.relief ? "url(#" + RELIEF_ID + ")" : "none";
    var onMark = t.relief && t.relief.target === "mark";
    s.setProperty("--splash-mark-filter", t.markFilter || (onMark ? reliefUrl : "none"));
    s.setProperty("--splash-signal-filter", t.signalFilter || (t.relief ? reliefUrl : "none"));
    if (t.strike !== "none") root.classList.add("splash--strike-" + t.strike);

    root.appendChild(div("splash-lamp"));
    root.appendChild(div("splash-vignette"));
    root.appendChild(div("splash-grain"));

    var stage = div("splash-stage");
    var frame = div("splash-rise splash-rise--1");
    var plinth = div("splash-plinth");
    // The material first, in the entry's own order, then the sheen — the one layer the
    // approved seven use — then the composition on top of both.
    for (var m = 0; m < t.materials.length; m++) plinth.appendChild(materialLayer(t.materials[m]));
    if (t.sheen) plinth.appendChild(div("splash-sheen"));

    var logoWrap = div("splash-rise splash-rise--2");
    logoWrap.appendChild(svgMark(LOGO, "splash-logo", t.gild, t.relief));
    plinth.appendChild(logoWrap);

    var wordWrap = div("splash-rise splash-rise--3");
    wordWrap.appendChild(svgMark(WORDMARK, "splash-wordmark", null, null));
    plinth.appendChild(wordWrap);

    var footWrap = div("splash-rise splash-rise--4");
    var foot = div("splash-foot");
    foot.appendChild(div("splash-rule"));
    var line = div("splash-line");
    line.textContent = LINE;
    line.style.setProperty("font-size", LINE_SIZE);
    line.style.setProperty("letter-spacing", LINE_TRACKING);
    foot.appendChild(line);
    footWrap.appendChild(foot);
    plinth.appendChild(footWrap);

    frame.appendChild(plinth);
    // THE BAR IS UNDER THE PLINTH AND EXACTLY AS WIDE AS IT. The rising frame becomes a
    // stretch column so the track takes the plinth's width at every window size - the same
    // arrangement the approved mockups use, and the reason the two edges line up.
    frame.style.setProperty("display", "flex");
    frame.style.setProperty("flex-direction", "column");
    frame.style.setProperty("align-items", "stretch");
    frame.appendChild(buildBar(t.bar));
    stage.appendChild(frame);
    root.appendChild(stage);
    // ROUND 74: the number, his company's line, the seal. A throw in there leaves the screen
    // exactly as it was drawn above.
    if (RO) {
      try {
        RO.dress(root, entry, {
          elapsed: function () {
            return shownAt === null ? 0 : clock() - shownAt;
          },
          held: function () {
            return heldMoment !== null;
          },
          hold: holdMs,
          // Exports and share cards make no sound. Native playback owns its cue.
          silent: drawing() || !!silent,
          drawing: drawing(),
          exportScale: drawing() ? window.__RICHOS_LAUNCH__.exportScale : 0,
          card: drawing() && window.__RICHOS_LAUNCH__.card === true
        });
      } catch (_e) {
        /* drawn without the additions */
      }
    }
    return root;
  }

  // -------------------------------------------------------------------------------------
  // The surface's life
  // -------------------------------------------------------------------------------------

  // ROUND 74: everything round 74 adds lives in `openings.js`, which loads before this file.
  // Without it this file behaves exactly as it did.
  var RO = window.RichOpenings || null;
  if (RO) RO.arrow = LOGO.paths[1].d;
  var replaying = null; // the gallery's Play: the callback to run when the screen has left
  var replayKept = null; // what `state` said about the start, while a replay is up

  var node = null;
  var yielded = false;
  var nativeRecovering = false;
  var ceiling = null;
  var holdTimer = null;
  var state = {
    shown: false,
    variationId: null,
    reason: null,
    /// Why nothing was drawn, when nothing was. A surface that declines to appear should
    /// be able to say why without anyone attaching a debugger.
    declined: null,
    /// What kind of start this was, as `start()` read it - `"fresh"`, `"crash-restart"` or
    /// `"second-window"`. `null` until `start()` has run. Reported rather than inferred
    /// from `shown`, because "no splash because he never quit" and "no splash because the
    /// library would not draw" are two different answers and a surface should be able to say which.
    kind: null,
    /// WHICH START THIS IS, as the shell reported it - `1`, `2`, `3`... or `null` when
    /// nobody said. Reported rather than derived from `variationId`, because "start 4, which
    /// shows #1" and "nobody told me, so #1" are two different answers.
    ordinal: null,
    /// How long this screen was given, in seconds. The CEO's default unless the entry that
    /// was drawn asked for its own.
    seconds: null,
    /// THE BAR'S OWN ACCOUNT OF ITSELF, and it exists for one reason: *"The animation is
    /// only supposed to happen ONCE. NO LOOPING."*
    ///
    /// `barPasses` counts the times the bar has reached the end. It is 0 while it runs and 1
    /// after it lands, and it can only be more than 1 if something restarted it.
    /// `barStopped` is true once `tick()` has returned without asking for another frame.
    ///
    /// WHY THIS IS REPORTED RATHER THAN INFERRED FROM THE WIDTH. A loop is only visible from
    /// outside during the window between the bar landing and the curtain leaving, which is
    /// the ceiling's one second of grace — and a restart beginning a moment after that window
    /// is invisible to any amount of watching. It WAS: the acceptance suite's first version
    /// of this proof watched the width for 3.95s, and a restart injected at the end of the
    /// landing flare went through it completely undetected. These two numbers close that
    /// hole, because a loop cannot leave the loop stopped.
    barPasses: 0,
    barStopped: false,
    /// ROUND 85: how many presses and keys the screen was given and did nothing with. It is
    /// here so a walk can show the screen was really pressed while it played.
    ignored: 0
  };

  function removeSelf() {
    if (raf !== null) {
      cancelAnimationFrame(raf);
      raf = null;
    }
    bar = null;
    if (node && node.parentNode) node.parentNode.removeChild(node);
    node = null;
    // ROUND 74
    if (RO) RO.stop();
    if (replaying) {
      var after = replaying;
      replaying = null;
      if (replayKept) {
        state.barPasses = replayKept.barPasses;
        state.barStopped = replayKept.barStopped;
        holdMs = replayKept.hold;
        replayKept = null;
      }
      after();
    }
  }

  function afterFade() {
    var finishedLaunch = !replaying && !drawing();
    state.native = false;
    removeSelf();
    var shell = window.__TAURI_INTERNALS__;
    if (finishedLaunch && shell && typeof shell.invoke === "function") {
      Promise.resolve(shell.invoke("splash_did_finish")).catch(function () {});
    }
  }

  // -------------------------------------------------------------------------------------
  // ROUND 85: NOTHING THE USER DOES CHANGES THE SCREEN
  // -------------------------------------------------------------------------------------
  //
  // The CEO, 2026-11-16: "We no longer need any interactivity in splash screens i.e. each splash
  // screen shall play from start to finish regardless of what the user might do with their
  // mouse."
  //
  // WHAT WAS HERE: the space key held the screen and showed the settings button (CEO §62), and
  // any other key, and any click, sent the screen away at once. All of that is removed: there
  // is no held state, no way to leave early, and so no settings button on this screen at all.
  //
  // WHAT IS HERE INSTEAD, and it is small. The pointer is the curtain's own for as long as it
  // is up (`splash.css`), so a click lands on it and presses nothing under it; and while it is
  // up no press, scroll or key travels any further than this function, because the app under
  // the curtain would otherwise act on it for somebody who cannot see it (Space and Enter open
  // Talk to Rich on the home screen, ⌘K opens the search). A key with ⌘ or ⌃ keeps what the Mac
  // itself does with it, quitting and reloading included: only the app's own listeners are
  // kept from it.
  var QUIET = ["keydown", "keyup", "keypress", "pointerdown", "pointerup", "mousedown", "mouseup", "click", "dblclick", "auxclick", "contextmenu", "wheel"];
  function quiet(e) {
    if (!node || yielded) return;
    e.stopImmediatePropagation();
    var key = e.type.indexOf("key") === 0;
    if (key && !e.metaKey && !e.ctrlKey) e.preventDefault();
    if ((e.type === "keydown" && !e.repeat) || e.type === "pointerdown") state.ignored++;
  }
  function listen(on) {
    var f = on ? window.addEventListener : window.removeEventListener;
    for (var i = 0; i < QUIET.length; i++) f.call(window, QUIET[i], quiet, true);
  }

  /// Get out of the way. Called by `main.js` when the app is usable and by the ceiling -
  /// whichever happens first, and only once. ROUND 85: never by the user's hand.
  function yieldNow(reason) {
    if (yielded || nativeRecovering || (state.native && reason !== "native-finished")) return;

    // THE HOLD, AND THE ONE CALLER IT APPLIES TO.
    //
    // `app-ready` is the ceremony finishing early: the app became usable before the
    // composition had had its three seconds. That one waits out the remainder, so the screen
    // plays to its end. The ceiling goes now: a failsafe that could itself be deferred is not
    // one.
    if (reason === "app-ready") {
      var left = holdRemaining();
      if (left > 0) {
        if (holdTimer === null) {
          holdTimer = setTimeout(function () {
            holdTimer = null;
            yieldNow("held");
          }, left);
        }
        return;
      }
    }

    yielded = true;
    if (holdTimer !== null) {
      clearTimeout(holdTimer);
      holdTimer = null;
    }
    // ROUND 74: a replay is not the start; how it left is its own reading.
    if (replaying) state.replayReason = reason;
    else state.reason = reason;
    if (ceiling !== null) {
      clearTimeout(ceiling);
      ceiling = null;
    }
    listen(false);
    if (!node) return;
    // Pin the composition where it was going BEFORE fading it, so a launch that finishes
    // mid-ceremony shows a finished mark on its way out rather than unlit metal.
    // ROUND 74: a bar that had landed is handed to the home screen; one cut short is not.
    if (state.native) {
      node.classList.add("splash--yielding");
      if (RO) RO.handover(node, true);
      Promise.resolve(window.__TAURI_INTERNALS__.invoke("splash_native_dismiss", { animated: true })).then(afterFade);
    } else {
      var hadLanded = !!(bar && bar.landed);
      node.classList.add("splash--settled");
      settleBar();
      if (RO) {
        RO.settle();
        if (hadLanded && !replaying) RO.handover(node);
      }
      node.classList.add("splash--yielding");
      setTimeout(afterFade, FADE_MS + 40);
    }
    // Drop the always-dark clamp as the curtain goes, not after it has gone: the fade is
    // the app arriving, and the control crossing over with it is the same "one system, two
    // lightings" move every other element makes.
    if (window.RichTheme) window.RichTheme.forceDark(false);
  }

  // ROUND 74. THE GALLERY'S PLAY. This file used to have no path that showed the screen a
  // second time, by design. This is that path, and it is the only one: the same build and the
  // same clock, for exactly the screen's own time, and (ROUND 85) as deaf to the pointer and
  // the keys as a start is. It is asked for by a person pressing Play; nothing else calls it.
  // A replay stays in the page and reports nothing to the launch tracker.
  function replay(openingId, done) {
    if (node || !RO) return false;
    var candidates = pool();
    var entry = null;
    for (var i = 0; i < candidates.length; i++) {
      if (candidates[i].id === RO.entryId({ id: openingId })) entry = candidates[i];
    }
    if (!entry) return false;
    yielded = false;
    heldMoment = null;
    state.barStopped = false;
    state.heldAt = null;
    // The start's own record is kept and put back when the replay has left: a replay is not
    // a start, and what `state` says about the launch stays what happened at the launch.
    replayKept = { barPasses: state.barPasses, barStopped: state.barStopped, hold: holdMs };
    holdMs = Math.round(secondsFor(entry) * 1000);
    replaying = typeof done === "function" ? done : function () {};
    try {
      node = build(entry);
      document.body.insertBefore(node, document.body.firstChild);
    } catch (e) {
      removeSelf();
      return false;
    }
    state.replayed = entry.id;
    state.replays = (state.replays || 0) + 1;
    shownAt = clock();
    if (bar) raf = requestAnimationFrame(tick);
    if (window.RichTheme) window.RichTheme.forceDark(true);
    listen(true);
    ceiling = setTimeout(function () {
      yieldNow("ceiling");
    }, holdMs);
    return true;
  }

  // -------------------------------------------------------------------------------------
  // FRAME EXPORT
  // -------------------------------------------------------------------------------------

  /// Every animation on this surface, read off the document: the CSS stages, the strike, the
  /// rule and the bar's fade, and whatever a later entry adds.
  function splashAnimations() {
    if (!node || !document.getAnimations) return [];
    return document.getAnimations().filter(function (a) {
      var t = a.effect && a.effect.target;
      return !!t && (t === node || node.contains(t));
    });
  }

  /// HOLD the splash at `ms` after its first frame: everything on it stands exactly where the
  /// running splash stands at that moment, painted now. True when there is a splash to hold.
  function holdAt(ms) {
    if (!node || shownAt === null) return false;
    var t = Number(ms);
    if (!isFinite(t) || t < 0) t = 0;
    heldMoment = t;
    state.heldAt = t;
    if (state.native) return true;
    if (raf !== null) {
      cancelAnimationFrame(raf);
      raf = null;
    }
    var landing = RO ? RO.seekLanding(t) : null;
    var all = splashAnimations();
    for (var i = 0; i < all.length; i++) {
      try {
        all[i].pause();
        all[i].currentTime = landing !== null && /^ro-land-/.test(all[i].animationName) ? t - landing : t;
      } catch (_e) {
        /* Skip an animation WebKit cannot seek. */
      }
    }
    paintMoment(t);
    return true;
  }

  /// THE SHELL'S PICK, from the object it puts in front of every page script, or `null`. The
  /// rules run in the shell at launch (`richos-core` `launch::pick_splash`); this page
  /// draws what was picked and no longer chooses, except where nothing was injected or the id is
  /// one it does not know (`choose()` below is that fallback).
  function shellPick() {
    try {
      var s = window.__RICHOS_LAUNCH__ && window.__RICHOS_LAUNCH__.splash;
      if (s && typeof s.id === "string" && s.id) return s;
    } catch (_e) {
      /* fall through */
    }
    return null;
  }

  /// Exporting a splash or a share card: build the picked splash, hold it at
  /// its first moment, and nothing else: no listeners, no ceiling, no report.
  function drawing() {
    try {
      return !!(window.__RICHOS_LAUNCH__ && window.__RICHOS_LAUNCH__.draw === true);
    } catch (_e) {
      return false;
    }
  }

  // The native layer owns every visible splash frame. The page keeps the covered
  // state until the shell confirms that the layer's fade has finished.
  function playNative(entry) {
    var invoke = window.__TAURI_INTERNALS__.invoke.bind(window.__TAURI_INTERNALS__);
    Promise.resolve(invoke("splash_native_finished")).then(function (result) {
      if (result.state === "finished" || result.elapsedMs >= holdMs) {
        holdAt(holdMs);
        yieldNow("native-finished");
      } else {
        recover(Math.max(0, result.elapsedMs), result.nativeCue === true);
      }
    }, function () { recover(0); });

    function recover(elapsed, nativeCue) {
      // Paint the fallback at the same point while the native view still covers it.
      // If the native clock started, it owns the cue through a video failure.
      if (!node || yielded) return;
      nativeRecovering = true;
      removeSelf();
      state.native = false;
      shownAt = null;
      heldMoment = null;
      state.heldAt = null;
      node = build(entry, nativeCue);
      document.body.insertBefore(node, document.body.firstChild);
      shownAt = clock();
      holdAt(elapsed);
      Promise.resolve(invoke("splash_native_dismiss", { animated: false })).then(resume, resume);

      function resume() {
        if (!node || yielded) return;
        nativeRecovering = false;
        heldMoment = null;
        state.heldAt = null;
        shownAt = clock() - elapsed;
        if (bar && barRun(elapsed) === 1) {
          bar.landed = true;
          bar.landedAt = shownAt + BAR_START_MS + barSpan();
        }
        splashAnimations().forEach(function (animation) {
          if (animation.currentTime < animation.effect.getComputedTiming().endTime) animation.play();
        });
        if (bar) raf = requestAnimationFrame(tick);
        ceiling = setTimeout(function () { yieldNow("ceiling"); }, holdRemaining() + CEILING_GRACE_MS);
        yieldNow("app-ready");
      }
    }
  }

  function start() {
    // The launch kind is checked BEFORE the library is read: a crash-restart must cost
    // nothing at all, not "a pool built and then thrown away".
    var kind = launchKind();
    if (kind !== null && kind !== KIND_FRESH) {
      state.kind = kind;
      state.declined = "not a fresh launch (" + kind + ")";
      return;
    }
    state.kind = kind === null ? KIND_FRESH : kind;
    var candidates = pool();
    if (!candidates.length) {
      // No library, an unparseable one, or every entry malformed. All three are the same
      // answer: a normal launch, with nothing drawn.
      state.declined = "no usable variation in the library";
      return;
    }
    var ordinal = launchOrdinal();
    state.ordinal = ordinal;
    var picked = shellPick();
    var entry = null;
    for (var c = 0; picked && c < candidates.length; c++) {
      if (candidates[c].id === picked.id) entry = candidates[c];
    }
    state.pickedBy = entry ? "shell" : "page";
    if (!entry) entry = choose(candidates, ordinal);
    state.native = !!(picked && picked.native && state.pickedBy === "shell" &&
      window.__TAURI_INTERNALS__ && typeof window.__TAURI_INTERNALS__.invoke === "function");
    holdMs = Math.round(secondsFor(entry) * 1000);
    state.seconds = holdMs / 1000;
    try {
      if (state.native) {
        // The native overlay owns the picture. The empty node keeps Home
        // covered until the fade finishes or the page takes over recovery.
        node = div("splash splash--native");
        node.id = "splash";
        node.setAttribute("aria-hidden", "true");
        node.dataset.variation = entry.id;
      } else {
        node = build(entry);
      }
      document.body.insertBefore(node, document.body.firstChild);
    } catch (e) {
      removeSelf();
      state.declined = "the chosen variation would not render: " + ((e && e.message) || e);
      return;
    }
    state.shown = true;
    state.variationId = entry.id;
    shownAt = clock();
    if (drawing()) {
      // The exporter samples this timeline at fixed times.
      state.drawing = true;
      if (window.__RICHOS_LAUNCH__.card === true) {
        // ROUND 74, "Save image": the same hidden page, held at the finished frame with the
        // line that names the opening, for one picture (`splash_opening.rs` `save_card`).
        node.classList.add("splash--settled");
        node.classList.add("splash--card");
        if (RO) RO.settle();
        holdAt(holdMs);
        state.card = true;
        return;
      }
      holdAt(0);
      return;
    }
    if (window.RichTheme) window.RichTheme.forceDark(true);
    listen(true);
    if (state.native) {
      holdAt(0);
      playNative(entry);
    } else {
      if (bar) raf = requestAnimationFrame(tick);
      ceiling = setTimeout(function () {
        yieldNow("ceiling");
      }, holdMs + CEILING_GRACE_MS);
    }
  }

  // THE PAGE HAS TAKEN OVER (Sage's plan 2026-11-14, rules 3 and 5): two animation frames after
  // this script ran, the page, with its splash if one is drawn, has painted at least once. The
  // shell then starts what waited for the page and writes its launch line. Reported whether or
  // not a splash is drawn; the shell counts the first report of a launch. No shell, no report.
  function reportPainted() {
    if (drawing()) return;
    if (!window.__TAURI_INTERNALS__ || typeof window.__TAURI_INTERNALS__.invoke !== "function") return;
    requestAnimationFrame(function () {
      requestAnimationFrame(function () {
        try {
          Promise.resolve(window.__TAURI_INTERNALS__.invoke("first_frame_painted")).catch(function () {});
        } catch (e) {
          /* the shell's ceiling starts the waiting work and writes the line instead */
        }
      });
    });
  }

  window.RichSplash = {
    yieldNow: yieldNow,
    state: state,
    /// Every entry the library offers that this renderer would actually draw. `main.js`
    /// does not use it; the acceptance suite does, to prove the pool it draws from is the
    /// pool that is shipped.
    pool: pool,
    /// The wire string for the one kind of start that draws, exported so the acceptance
    /// suite and `main.rs`'s `LaunchKind::as_str` can be checked against one another rather
    /// than both against a literal somebody typed twice.
    KIND_FRESH: KIND_FRESH,
    /// The CEO's default, and the ceiling on a per-screen override. Exported so the
    /// acceptance suite measures the surface against the number the surface actually holds
    /// rather than against one typed a second time in a test.
    SPLASH_SECONDS: SPLASH_SECONDS,
    MAX_SPLASH_SECONDS: MAX_SPLASH_SECONDS,
    /// The rule, exported so it can be driven directly against a pool the suite supplies.
    /// It is a pure function of the pool and the ordinal; that is what makes it testable
    /// without launching anything, and what makes the later criteria a change to one place.
    choose: choose,
    /// ROUND 74: the gallery's Play, and whether a screen is up.
    replay: replay,
    isUp: function () {
      return !!node;
    },
    /// Build-time export and Save Image sample the same authored clock.
    holdAt: holdAt,
    /// Where the splash stands now, in ms after its first frame (`null` before it is up): the
    /// reading the bar and the hold are painted from. Read-only; the suite compares by it.
    moment: function () {
      return shownAt === null ? null : clock() - shownAt;
    }
  };

  start();
  reportPainted();
})();

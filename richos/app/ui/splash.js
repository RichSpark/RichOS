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
//   * it does no I/O, reads no file, and awaits no command before painting. The setting it
//     needs is read synchronously out of `localStorage` - the same instant-local-cache
//     pattern `main.js` already uses for the assertiveness dial, for the same reason, with
//     the Rust `ConfigStore` as the durable source of truth reconciled afterwards;
//   * the curtain is `pointer-events: none` for its whole life, so the live app underneath
//     is reachable the moment it is live;
//   * and it gets out of the way on the FIRST of three signals - the app reporting ready,
//     the CEO touching a key or the trackpad, or a hard ceiling - so no failure anywhere
//     else in the shell can leave it up.
//
// THE HONEST STATES. If the switch is off, if the library is missing, unparseable or
// empty, if every entry in it is malformed, or if anything at all throws while drawing,
// the result is the same and it is deliberate: NO SPLASH, and a completely normal launch.
// Never a half-drawn frame, never a fallback that looks like a broken app. `removeSelf()`
// below is what makes a partial build leave nothing behind.
//
// WHAT IS NOT HERE, named rather than hidden: tier 2 (a true line about his record) and
// tier 3 (hallmarks) from `richos-splash-micro-game-2026-08-30.md`. Both wait on CEO
// answers to that document's §8. Their seams are marked in `splash.css` - the slot under
// the rule, and the plinth's inner keyline - and neither needs this file redesigned.

"use strict";

(function () {
  // The one key this surface owns in the webview's local store: a mirror of the Rust
  // `ConfigStore` (durable, authoritative, reconciled by `main.js` after boot), read
  // synchronously because this file has to decide whether to draw before it can await
  // anything.
  //
  // `richos.splash.last` used to live here as well - the no-immediate-repeat guard for the
  // random draw. The draw is gone (see `choose()`), so the guard is gone with it: a rule
  // that says "the second start shows #2" has nothing to remember between launches.
  /// The clock the hold is measured on. `performance.now()` is monotonic and immune to a
  /// wall-clock adjustment mid-launch; `Date.now()` is the fallback. Neither is stored and
  /// neither is displayed, so no timezone question arises here - the UTC ruling applies to
  /// the durable stamps in `config.rs`, which are epoch millis and therefore already UTC.
  var now = function () {
    return window.performance && window.performance.now ? window.performance.now() : Date.now();
  };

  // -------------------------------------------------------------------------------------
  // THE SPACE KEY'S CLOCK — ONE OFFSET, ON THE ONE CLOCK EVERYTHING HERE ALREADY SHARES
  // -------------------------------------------------------------------------------------
  //
  // CEO §62, 2026-09-19: *"Hitting the space key while the splash screen is shown should
  // 'pause' the splash screen."* Amended the same day: *"if the animation on the splash
  // screen can be easily paused without adding too much complexity, then sure the animation
  // should pause."*
  //
  // IT CAN, AND THIS IS WHY IT IS THREE LINES RATHER THAN A STATE MACHINE. Everything that
  // moves on this surface is driven from ONE reading: `clock()`. The bar's fill is
  // `clock() - shownAt`, the landing flare is `clock() - bar.landedAt`, and the hold the
  // ceremony is measured against is `holdMs - (clock() - shownAt)` - the same number, which
  // is the property `SPLASH_SECONDS`' comment already names ("the bar reaches 100% at
  // exactly this many seconds"). Stop that one reading and all three stop together, in step,
  // with no second notion of time to drift against the first.
  //
  // `pausedTotal` is how long the surface has been held, in ms, and `pausedSince` is the raw
  // reading the current hold began at. While a hold is on, `clock()` returns the instant it
  // began; when the hold ends, the time spent is added to the offset, so the value `clock()`
  // returns is continuous across the resume and never goes backwards. The RAW clock is still
  // what a pause is measured with - a paused clock cannot time its own pause.
  //
  // The CSS composition (four `splash-rise` stages, the strike and the rule) is not on this
  // clock and does not need to be: it is paused declaratively by `splash--paused` in
  // `splash.css`, and the bar track's one Web-Animations fade is paused through the handle
  // `buildBar` keeps on it. Three mechanisms because there are three kinds of motion, and
  // each one is paused where it lives.
  var pausedTotal = 0;
  var pausedSince = null;
  function clock() {
    return (pausedSince === null ? now() : pausedSince) - pausedTotal;
  }

  var KEY_ENABLED = "richos.splash.enabled";

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
  // underneath is live, focused and reachable from its first frame, and this curtain is
  // `pointer-events: none` for its whole life - so holding it is not a wait, it is a layer
  // that has not faded yet. The two things that prove it are still true and still tested:
  // his first keystroke or touch yields IMMEDIATELY (the hold is never allowed to catch his
  // hand), and the ceiling still fires on a launch that never reports ready.

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

  /// How much of the hold is left, in ms. Zero once it is served - and zero for any caller
  /// that is not the app-ready path, because the hold governs the ceremony and never the
  /// CEO's hand.
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
  var FLARE_MS = 950;
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
  // Storage, which is allowed to be unavailable
  // -------------------------------------------------------------------------------------

  function read(key) {
    try {
      return window.localStorage.getItem(key);
    } catch (_e) {
      return null;
    }
  }

  function write(key, value) {
    try {
      window.localStorage.setItem(key, value);
    } catch (_e) {
      /* A webview with storage denied still gets a splash; it just gets no repeat guard. */
    }
  }

  /// ABSENT MEANS ON. An absent key is an absent opinion, not a decision to switch the
  /// surface off - the same rule `config.rs` states on the Rust side, and the two have to
  /// agree or a first launch would disagree with itself.
  /// THE DURABLE ANSWER FIRST, THE MIRROR ONLY WHEN THERE ISN'T ONE — audit-8 row 5.
  ///
  /// This read used to be `read(KEY_ENABLED) !== "false"` and nothing else, and it decided the
  /// curtain in the `<head>`, before any command could answer. The CEO switched the opening
  /// screen off, `config.json` recorded it, and it came back on the next two launches.
  ///
  /// THE MIRROR IS A CACHE AND IT IS SHARED BETWEEN INSTALLS THAT DISAGREE. WebKit keys its
  /// website data by BUNDLE ID and ignores `$HOME`, so a candidate walked under a QA scratch
  /// `HOME` shares one `localStorage` with the CEO's installed app while reading a different
  /// `config.json` — and `main.js`'s `syncSplashFromBackend` writes each instance's own durable
  /// answer into that one shared mirror at the end of every boot. Last writer wins, and the
  /// loser draws a curtain its owner switched off.
  ///
  /// `window.__RICHOS_LAUNCH__.splashEnabled` is the durable answer, put in front of every page
  /// script by the Tauri initialization script that already carries the launch kind. `null`
  /// means this process could not read the store, and only then does the mirror decide — which
  /// is exactly today's behavior, so the degraded path is no worse than what shipped.
  ///
  /// The mirror stays: it is what `main.js`'s settings row reads synchronously to paint the
  /// switch, and it is the only answer a page with no Tauri behind it (the acceptance suite,
  /// a browser) can have.
  function durableEnabled() {
    try {
      var launch = window.__RICHOS_LAUNCH__;
      return launch && typeof launch.splashEnabled === "boolean" ? launch.splashEnabled : null;
    } catch (e) {
      return null;
    }
  }

  function enabled() {
    var durable = durableEnabled();
    if (durable !== null) return durable;
    return read(KEY_ENABLED) !== "false";
  }

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
  // WHERE THE LATER CRITERIA ATTACH: this function, and only this function. It already takes
  // the whole pool and the ordinal; a criteria-driven pick takes the same two arguments plus
  // whatever the criterion reads, and nothing else in this file changes. The pool is an
  // ARRAY and the rule reads its length, so a third screen is a third object in the library.
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
      /// The track's own fade-in, kept rather than fired and forgotten: it is the one piece
      /// of motion on this surface that is neither a CSS animation (which `splash--paused`
      /// reaches) nor on `clock()` (which the space key stops), so pausing it needs its
      /// handle. `null` under reduced motion, where it is never started.
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

  /// ONE PASS, AND THEN IT STOPS. The only exits that do not schedule another frame are the
  /// two that call `stopTicking()`, and there is no other caller of `tick`.
  function tick() {
    if (!bar) return;
    var span = holdMs - BAR_START_MS;
    if (span < 350) span = 350;
    var u = (clock() - shownAt - BAR_START_MS) / span;
    if (u < 0) u = 0;
    if (u > 1) u = 1;
    paintBar(u + BAR_SURGE * Math.sin(2 * Math.PI * u) * (1 - u));

    if (u >= 1 && !bar.landed) {
      bar.landed = true;
      bar.landedAt = clock();
      state.barPasses++;
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


  function build(entry) {
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
    return root;
  }

  // -------------------------------------------------------------------------------------
  // The surface's life
  // -------------------------------------------------------------------------------------

  var node = null;
  var yielded = false;
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
    /// from `shown`, because "no splash because he never quit" and "no splash because he
    /// switched it off" are two different answers and a surface should be able to say which.
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
    /// THE SPACE KEY'S OWN ACCOUNT OF ITSELF (CEO §62). `paused` is the state the settings
    /// button appears in and the state the automatic switch to the home screen is held in;
    /// `pauses` counts how many times it has been entered, so "he held it once" and "the
    /// key is toggling on its own" are different readings; `pausedMs` is how long has been
    /// spent held, which is exactly how much later than its three seconds this screen will
    /// leave. `deferred` is the automatic caller that asked to go while it was held and was
    /// refused - reported rather than swallowed, because "the app was ready and waited" and
    /// "the app never became ready" are two different launches.
    paused: false,
    pauses: 0,
    pausedMs: 0,
    deferred: null
  };

  function removeSelf() {
    if (raf !== null) {
      cancelAnimationFrame(raf);
      raf = null;
    }
    bar = null;
    if (node && node.parentNode) node.parentNode.removeChild(node);
    node = null;
  }

  // -------------------------------------------------------------------------------------
  // THE SPACE KEY — HOLD THE SCREEN, AND ONLY THEN OFFER SETTINGS
  // -------------------------------------------------------------------------------------
  //
  // CEO §62, 2026-09-19, verbatim and in full because it is the whole specification of this
  // block: *"The splash screen is one of those rare cases where the screen should NOT have a
  // settings button (because it's only there for 3 seconds by default and 5 seconds in some
  // cases in the future). However: An additional feature for the splash screen should be
  // this: Hitting the space key while the splash screen is shown should 'pause' the splash
  // screen. Not necessarily pause the animation but stop it from automatically switching
  // from splash screen to home screen. Hitting the space key a second time would 'resume'
  // it i.e. switch to home screen. AND: while the splash screen is 'paused', THAT'S when the
  // settings button should show up on the splash screen. Because that's the only time it
  // would make sense. Otherwise no settings button on the splash screen."*
  //
  // THIS REPLACES §15's EXCEPTION ON THIS ONE SURFACE, and the replacement is the ruling's
  // own: §15 puts the settings button on every screen, and this is the screen he has taken
  // it off. `splash.css` is where that happens (the button's own CSS is in `style.css` and
  // is not touched), keyed on the curtain being present, not paused and not yet yielding -
  // so the button cannot be left hidden by any state this file can end in, including a
  // throw. The moment the node is gone the rule cannot match.
  //
  // WHAT IS PAUSED, AND WHAT IS NOT. The automatic switch to the home screen is: the ceiling
  // and `main.js`'s `app-ready` yield both stop taking the curtain down (`yieldNow` below).
  // HIS HAND IS NOT: any other key, and any pointer, still dismisses on first input exactly
  // as before, held or not - §5.5 forbids anything on this surface that is delaying, and a
  // hold he can get stuck in would be precisely that. The animation is paused too, per the
  // same day's amendment, because it could be done in three places rather than a redesign.
  var paused = false;

  /// The space bar, and only the space bar, and only unmodified. A modified press (⌘, ⌃, ⌥)
  /// is an app or system shortcut rather than this surface's control, so it falls through to
  /// the first-input dismissal every other key has always taken.
  function isSpace(e) {
    if (e.metaKey || e.ctrlKey || e.altKey) return false;
    return e.key === " " || e.key === "Spacebar" || e.code === "Space" || e.keyCode === 32;
  }

  /// HOLD IT. The automatic switch stops; the ceremony stops where it stands; the settings
  /// button appears.
  function pauseNow() {
    if (paused || yielded || !node) return;
    paused = true;
    state.paused = true;
    state.pauses++;
    // The clock stops FIRST, so the bar and the hold both freeze on the same reading rather
    // than on two readings taken either side of the work below.
    pausedSince = now();
    // The two automatic ways out, disarmed rather than left to fire into a `yieldNow` that
    // would refuse them. The ceiling is not re-armed on resume because resume IS the yield -
    // a failsafe against a boot that hangs has nothing to protect while he is holding the
    // screen open on purpose, and one more keystroke takes it down whatever the app is doing.
    if (ceiling !== null) {
      clearTimeout(ceiling);
      ceiling = null;
    }
    if (holdTimer !== null) {
      clearTimeout(holdTimer);
      holdTimer = null;
      state.deferred = "app-ready";
    }
    node.classList.add("splash--paused");
    if (bar && bar.enter && bar.enter.pause) {
      try {
        bar.enter.pause();
      } catch (_e) {
        /* an engine that will not pause its own animation still gets a paused surface */
      }
    }
    // The frame loop is put down rather than left spinning on a frozen number: a hold has no
    // end he is obliged to reach, and a `requestAnimationFrame` that repaints an unchanged
    // bar for as long as he leaves the screen up is work with no picture to show for it.
    // `state.barStopped` is deliberately NOT set - the bar has not finished, it is held, and
    // that flag's whole job is to answer "did the one pass end".
    if (raf !== null) {
      cancelAnimationFrame(raf);
      raf = null;
    }
  }

  /// Let it go again. The clock picks up exactly where it stopped, so a screen held at 1.4 s
  /// still has 1.6 s of ceremony left in it and the bar is where he left it, not ahead of it.
  function unpause() {
    if (!paused) return;
    paused = false;
    state.paused = false;
    if (pausedSince !== null) {
      pausedTotal += now() - pausedSince;
      pausedSince = null;
      state.pausedMs = Math.round(pausedTotal);
    }
    if (node) node.classList.remove("splash--paused");
    if (bar && bar.enter && bar.enter.play) {
      try {
        bar.enter.play();
      } catch (_e) {
        /* see pauseNow */
      }
    }
    if (bar && !state.barStopped && raf === null) raf = requestAnimationFrame(tick);
  }

  /// The second space: *"resume it i.e. switch to home screen"*. Both halves, in his order -
  /// the ceremony is let go first so nothing is left frozen behind the fade, and then the
  /// curtain goes. `yieldNow` pins the composition to where it was going (`splash--settled`)
  /// as it always has, so what a resumed screen shows on its way out is a finished mark and
  /// not a frame that ran for 16 ms.
  function resumeNow() {
    unpause();
    yieldNow("resume");
  }

  /// The CEO's first keystroke or touch gets the curtain out of the way — with TWO
  /// exceptions now, and both of them are §62's.
  ///
  /// THE SETTINGS BUTTON. It is drawn ABOVE this curtain (z-index 300 in `style.css:4990`
  /// against `splash.css:21`'s 200) and, since §62, it is only on this screen while the
  /// screen is paused. Reaching it must not count as first input: the curtain would lift the
  /// instant it was touched, and the bug report the CEO was opening would start from the
  /// shell instead of from the opening screen he was actually looking at. That is precisely
  /// the outcome §15 names — "reporting a bug must never require navigating away from the
  /// screen the bug is on" — and it is why he put the button on the paused screen at all.
  /// The exemption covers the whole `.settings` wrapper, so the menu's own rows, and a space
  /// pressed on a focused row inside it, belong to the menu and never to this file.
  ///
  /// THE SPACE KEY. It holds the screen, and a second press resumes and goes. It is consumed
  /// — `preventDefault` and `stopPropagation` — because it is this surface's control while
  /// this surface is up: `main.js` has focused the composer by then (`main.js:7669`), and a
  /// space that paused the opening screen AND typed a character into the thread behind it
  /// would be one keystroke doing two things. A held-down key repeats; the repeats are
  /// swallowed rather than toggled on, because holding space is one press to a person.
  ///
  /// AND WHAT THE MENU'S BUST A BUG OPENS (round 21, CEO §115). The report opens in the Rich
  /// panel ON TOP of this held screen (`#bugdock`, z-index 298, the app's top layer), and the user
  /// answers it there: a click into it, and every key typed into it, spaces included, belongs to
  /// the report. Measured with this exemption taken out (`tests/bug-report.js`, "Bust a bug on
  /// the held opening screen"): the first click into the answer box took the screen the bug is on
  /// down. A first key would have too, and a space would have been eaten by the branch below as
  /// the hold's own toggle rather than typed.
  ///
  /// Everything else still dismisses on first input, unchanged, paused or not.
  function onInput(e) {
    var t = e && e.target;
    if (t && t.closest && t.closest(".settings, #bugdock")) return;
    if (e.type === "keydown" && isSpace(e)) {
      e.preventDefault();
      e.stopPropagation();
      if (e.repeat) return;
      if (paused) resumeNow();
      else pauseNow();
      return;
    }
    // ESCAPE TAKES THE CURTAIN DOWN AND NOTHING ELSE, and this is the third instance of one
    // defect rather than a new rule.
    //
    // MEASURED AT dcebed09, WebKit, on a Mac missing its engine: `init()` opens the engine
    // offer while this curtain is still up (z-index 200 over `.overlay`'s 60), so it is on
    // screen in the DOM and behind a full-screen picture to a person. `main.js`'s Escape
    // machinery asks `isOnScreen()`, which tests `hidden`, `display` and `visibility` and has
    // no idea what is painted over what — so one Escape here took the curtain down AND pressed
    // "Not now" on a question he had never seen. Result, measured: `#setup-sheet.hidden` true,
    // the offer gone for the session, and an app that looks perfectly normal and cannot answer
    // anything. That is Ray's candidate .15 (`esc-20260919T152225Z-2e44d112`), reached by the
    // key a person presses to get a curtain out of the way.
    //
    // WHY ONLY ESCAPE, and not every first input. His first LETTER must still reach the
    // composer — §5.5 forbids anything delaying on this surface, and swallowing the "h" of the
    // first thing he ever types would be a worse instrument than the one this fixes. Escape is
    // not content. On this surface it means exactly one thing, the curtain is already going,
    // and a dismissal that also dismisses something it is covering is one keystroke doing two
    // jobs — the same argument §62 makes two branches above for the space bar.
    if (e.type === "keydown" && e.key === "Escape" && !e.metaKey && !e.ctrlKey && !e.altKey) {
      e.preventDefault();
      e.stopPropagation();
      yieldNow("first-input");
      return;
    }
    yieldNow("first-input");
  }

  /// Get out of the way. Called by `main.js` when the app is usable, by the CEO's first
  /// keystroke or touch, and by the ceiling - whichever happens first, and only once.
  function yieldNow(reason) {
    if (yielded) return;

    // THE SPACE KEY'S HOLD, AND THE THREE CALLERS IT APPLIES TO (CEO §62).
    //
    // *"stop it from automatically switching from splash screen to home screen"* - so the
    // three callers that switch BY THEMSELVES are refused while the screen is held, and are
    // refused by name rather than by a default, because the list is the ruling: `app-ready`
    // is `main.js` reporting the shell usable, `held` is that same yield coming back after
    // the remainder of the hold, and `ceiling` is the failsafe. Nothing else is automatic.
    //
    // WHAT IS DELIBERATELY NOT IN THE LIST: `first-input` (his hand - §5.5), `resume` (his
    // hand again, and the whole point of the second press), and any caller a harness invents.
    // A hold that could swallow an explicit call would be a surface that can wedge, and this
    // one is `pointer-events: none` over a live app precisely so it never can.
    if (paused && (reason === "app-ready" || reason === "held" || reason === "ceiling")) {
      state.deferred = reason;
      return;
    }
    // Any other caller getting through while the screen is held takes the hold off on its
    // way past, so the clock, the class and `state.paused` cannot be left saying "held" on a
    // surface that has already decided to go.
    if (paused) unpause();

    // THE HOLD, AND THE ONE CALLER IT APPLIES TO.
    //
    // `app-ready` is the ceremony finishing early: the app became usable before the
    // composition had had its three seconds. That one waits out the remainder. Everything
    // else goes now, and the two that matter are exactly the two that must never be made to
    // wait - "first-input" is the CEO's hand, and §5.5 forbids anything on this surface that
    // is delaying; "ceiling" is the failsafe, and a failsafe that could itself be deferred
    // is not one.
    //
    // `yielded` is deliberately NOT set here, so a keystroke arriving during the hold still
    // takes the fast path and reports its own reason rather than being swallowed as a
    // duplicate.
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
    state.reason = reason;
    if (ceiling !== null) {
      clearTimeout(ceiling);
      ceiling = null;
    }
    window.removeEventListener("keydown", onInput, true);
    window.removeEventListener("pointerdown", onInput, true);
    if (!node) return;
    // Pin the composition where it was going BEFORE fading it, so a launch that finishes
    // mid-ceremony shows a finished mark on its way out rather than unlit metal.
    node.classList.add("splash--settled");
    settleBar();
    node.classList.add("splash--yielding");
    setTimeout(removeSelf, FADE_MS + 40);
    // Drop the always-dark clamp as the curtain goes, not after it has gone: the fade is
    // the app arriving, and the control crossing over with it is the same "one system, two
    // lightings" move every other element makes.
    if (window.RichTheme) window.RichTheme.forceDark(false);
  }

  function start() {
    if (!enabled()) {
      state.declined = "switched off";
      return;
    }
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
    var entry = choose(candidates, ordinal);
    holdMs = Math.round(secondsFor(entry) * 1000);
    state.seconds = holdMs / 1000;
    try {
      node = build(entry);
      document.body.insertBefore(node, document.body.firstChild);
    } catch (e) {
      removeSelf();
      state.declined = "the chosen variation would not render: " + ((e && e.message) || e);
      return;
    }
    state.shown = true;
    state.variationId = entry.id;
    shownAt = clock();
    // ONE PASS OF THE LOADING BAR, STARTING NOW. `shownAt` is the clock it runs on and the
    // clock the hold is measured against, so the bar reaches 100% at exactly the instant the
    // hold is served - the same number, not two numbers that agree.
    if (bar) raf = requestAnimationFrame(tick);
    // §15's ONE PERMANENT EXCEPTION: "because of its nature the start screen will always
    // need to be in dark mode", and no switch reaches it. The curtain's own composition was
    // always dark — it draws from `--splash-*` properties this file sets, never from the
    // app's theme tokens — but the SETTINGS BUTTON sits above the curtain (it has to; its
    // floor is "Bust a bug" from every screen) and it does read those tokens. Without this
    // clamp a CEO on light mode gets an ivory control floating on a midnight composition.
    //
    // `RichTheme` and not `RichSettings`: theme-boot.js is loaded in `<head>`, so it is
    // certain to exist here, whereas settings-button.js loads after this file. The clamp is
    // a FORCE flag and never writes the preference, so light mode is exactly where he left
    // it the moment the curtain lifts.
    if (window.RichTheme) window.RichTheme.forceDark(true);
    window.addEventListener("keydown", onInput, true);
    window.addEventListener("pointerdown", onInput, true);
    ceiling = setTimeout(function () {
      yieldNow("ceiling");
    }, holdMs + CEILING_GRACE_MS);
  }

  window.RichSplash = {
    yieldNow: yieldNow,
    state: state,
    /// The key, exported so `main.js` mirrors the durable Rust setting into the same place
    /// this file reads it from, rather than knowing the string twice.
    KEY_ENABLED: KEY_ENABLED,
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
    choose: choose
  };

  start();
})();

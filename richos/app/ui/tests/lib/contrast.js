// WCAG contrast, computed — the arithmetic and the DOM walk that `contrast.js` runs against
// the shipping shell.
//
// WHY THIS IS A LIBRARY AND NOT A SUITE. `run.js` discovers every `.js` in the tests
// directory as a suite; `lib/` is the one place shared code can live without being started
// as one. Two consumers need the SAME arithmetic: node, where it is checked against the
// published values of the calculator the standing rule names
// (https://webaim.org/resources/contrastchecker/), and WebKit, where it is applied to real
// computed styles. So the math is written ONCE here as ordinary functions and shipped into
// the page as its own source text (`pageScript()`), rather than being typed out a second
// time inside a `page.evaluate` body. A checker whose in-browser math is a COPY of the math
// its unit test proves is a checker with an unproven half.
//
// THE ONE RULE THAT SHAPES EVERY BRANCH BELOW: an unresolvable colour is a FAILURE TO PROVE,
// never a pass. A gradient behind a caption, a blend mode, a `background-clip: text`, a
// panel painted over the node, an unparseable colour notation — each of those is a place a
// lazy checker returns "fine" and moves on, and each is reported here as its own bucket that
// the suite fails on. The number that would embarrass this file is not the count of
// failures; it is a count of nodes quietly skipped.
//
// WHAT IT DOES NOT SEE, stated here rather than discovered later:
//
//   * `<canvas>` — pixels with no computed style. Nothing in this walk can read them. The
//     app's shipping shell contains none (asserted as a check, so the day one lands the
//     assertion is what tells you), but the round-7 and round-8.1 material studies are
//     canvas-heavy and this gate says NOTHING about them.
//   * SVG `fill`/`stroke` — a different colour property on a different box model. The
//     splash wordmark is SVG. Counted and named, never silently passed.
//   * text over a raster image, `text-shadow`, `-webkit-text-stroke` — reported unresolvable.
//   * paint order between two overlapping non-hit-testable layers.
//   * `::before`/`::after` generated text USED TO BE ON THIS LIST WITHOUT BEING WRITTEN ON
//     IT, which is worse than a gap: the walk collects `nodeType === 3` children and a
//     pseudo-element is not a node, so `content: "Settings"` on the settings button — text
//     §15 puts on every screen — was never measured and never counted, under a header whose
//     own thesis is that "the number that would embarrass this file is a count of nodes
//     quietly skipped". It is measured now, in its own `generated` bucket, and what STILL
//     cannot be measured from here is a pseudo-element that is positioned AND has no opaque
//     background of its own: it has no box to hit-test, so it is reported unprovable rather
//     than composited against a host it may not be painting over.

"use strict";

// ---------------------------------------------------------------------------------------
// The arithmetic (WCAG 2.2 §1.4.3 / §1.4.11)
// ---------------------------------------------------------------------------------------

/// `rgb()`/`rgba()` in either the comma or the space/slash notation, plus the `transparent`
/// keyword. ANY other notation returns null, and every caller treats null as unresolvable —
/// the safe direction. `color(display-p3 …)` reaching this function must not silently become
/// black.
function parseCssColor(str) {
  if (!str) return null;
  const s = String(str).trim().toLowerCase();
  if (s === "transparent") return { r: 0, g: 0, b: 0, a: 0 };
  const m = s.match(/^rgba?\(([^)]+)\)$/);
  if (!m) return null;
  const parts = m[1].split(/[,/\s]+/).filter(function (p) { return p.length; });
  if (parts.length < 3 || parts.length > 4) return null;
  const chan = function (p) { return p.indexOf("%") >= 0 ? (parseFloat(p) / 100) * 255 : parseFloat(p); };
  const r = chan(parts[0]);
  const g = chan(parts[1]);
  const b = chan(parts[2]);
  let a = 1;
  if (parts.length === 4) a = parts[3].indexOf("%") >= 0 ? parseFloat(parts[3]) / 100 : parseFloat(parts[3]);
  if (![r, g, b, a].every(function (v) { return typeof v === "number" && isFinite(v); })) return null;
  return { r: r, g: g, b: b, a: a };
}

function srgbToLinear(c) {
  const v = c / 255;
  return v <= 0.04045 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4);
}

function relativeLuminance(c) {
  return 0.2126 * srgbToLinear(c.r) + 0.7152 * srgbToLinear(c.g) + 0.0722 * srgbToLinear(c.b);
}

/// Composite a partly transparent colour over an opaque one. Straight alpha, which is what a
/// browser does for a `background-color: rgba(...)` over an opaque parent.
function compositeOver(fg, bg) {
  const a = fg.a;
  return {
    r: fg.r * a + bg.r * (1 - a),
    g: fg.g * a + bg.g * (1 - a),
    b: fg.b * a + bg.b * (1 - a),
    a: 1,
  };
}

function contrastRatio(fg, bg) {
  const l1 = relativeLuminance(fg);
  const l2 = relativeLuminance(bg);
  const hi = Math.max(l1, l2);
  const lo = Math.min(l1, l2);
  return (hi + 0.05) / (lo + 0.05);
}

/// TWO DECIMALS, because that is what the calculator the rule names reports, and because a
/// ratio of 4.499 and a ratio of 4.5 must not disagree between the tool a designer opens and
/// the gate that fails their branch. Comparisons downstream are made on the ROUNDED value
/// for exactly that reason.
function round2(x) {
  return Math.round(x * 100) / 100;
}

/// 18.66px bold, or 24px at any weight — WCAG's "large scale" in CSS pixels.
function isLargeText(fontSizePx, fontWeight) {
  const w = parseInt(fontWeight, 10);
  const bold = isFinite(w) ? w >= 700 : String(fontWeight) === "bold";
  return fontSizePx >= 24 || (bold && fontSizePx >= 18.66);
}

function hex(c) {
  const h = function (v) {
    const n = Math.max(0, Math.min(255, Math.round(v)));
    return (n < 16 ? "0" : "") + n.toString(16);
  };
  return "#" + h(c.r) + h(c.g) + h(c.b);
}

// ---------------------------------------------------------------------------------------
// A NON-TEXT INDICATOR MEASURED FROM THE PIXELS — audit-10 row 3
// ---------------------------------------------------------------------------------------

/// THE ONE THING THE DOM WALK BELOW STRUCTURALLY CANNOT DO: measure a control the PLATFORM
/// painted.
///
/// The walk reads `backgroundColor` and the four border colors. A bare `<input type="radio">`
/// with no CSS has `rgba(0,0,0,0)` and `0px` for all five, because the ring on screen is the
/// UA's own rendering and has no computed style anywhere — so the walk files it `textOnly`,
/// which is the bucket that means "nothing of its own to check". That is the correct answer to
/// the question the walk can ask, and it is why nine radios reached candidate .10 with the
/// light-mode unselected ring at 1.63:1 against a 3:1 floor.
///
/// So this measures the rendered frame instead, and it is the same method Ray's audits use, in
/// this file so the arithmetic is the arithmetic the suite already proves against WebAIM's
/// published values rather than a second copy:
///
///   * GROUND — the most common value in the crop's outer two-pixel skirt. The skirt is the
///     surface the control is drawn ON, so this resolves a ground rather than assuming one.
///   * INK — the value occurring at least `minSamples` times whose luminance is FURTHEST from
///     the ground. A three-sample floor is what keeps an antialiasing artifact from being
///     reported as the control's color; it is the rule Ray states in his own audits, and on the
///     one crop where nothing clears it (a count over moving artwork) the honest answer is that
///     the frequency method has nothing to measure against.
///   * `inkSamples` — HOW MUCH of that ink there is. This is the part a ratio cannot carry: a
///     filled disc and an empty ring of the same diameter and the same color have the same
///     ratio and opposite meanings, and it was the opposite meaning that shipped in dark mode.
function measureIndicatorCrop(pngBuffer, minSamples) {
  const png = require("./png");
  const img = png.decode(pngBuffer);
  const need = minSamples || 3;
  const at = function (x, y) {
    const o = (y * img.width + x) * 4;
    return { r: img.data[o], g: img.data[o + 1], b: img.data[o + 2], a: 1 };
  };
  const skirt = new Map();
  const all = new Map();
  for (let y = 0; y < img.height; y++) {
    for (let x = 0; x < img.width; x++) {
      const c = at(x, y);
      const k = hex(c);
      all.set(k, (all.get(k) || 0) + 1);
      const edge = x <= 1 || x >= img.width - 2 || y <= 1 || y >= img.height - 2;
      if (edge) skirt.set(k, (skirt.get(k) || 0) + 1);
    }
  }
  const groundKey = Array.from(skirt.entries()).sort(function (a, b) { return b[1] - a[1]; })[0][0];
  const toRgb = function (k) {
    return { r: parseInt(k.slice(1, 3), 16), g: parseInt(k.slice(3, 5), 16), b: parseInt(k.slice(5, 7), 16), a: 1 };
  };
  const ground = toRgb(groundKey);
  const gl = relativeLuminance(ground);
  let best = null;
  all.forEach(function (n, k) {
    if (n < need) return;
    const c = toRgb(k);
    const d = Math.abs(relativeLuminance(c) - gl);
    if (!best || d > best.d) best = { key: k, color: c, d: d, n: n };
  });
  if (!best) {
    // NOT A PASS. The same rule the walk follows: an unresolvable color is a failure to prove.
    return { ground: groundKey, ink: null, inkSamples: 0, ratio: 0, unresolvable: "no value in the crop occurs " + need + " times" };
  }
  return {
    ground: groundKey,
    ink: best.key,
    inkSamples: best.n,
    ratio: round2(contrastRatio(best.color, ground)),
    unresolvable: null,
  };
}

// ---------------------------------------------------------------------------------------
// The DOM walk — written here, executed in WebKit
// ---------------------------------------------------------------------------------------

/// Returns the whole probe as one self-installing script. `contrast.js` hands this to
/// `page.evaluate` after the surface has settled; it defines `window.__contrastProbe(opts)`
/// and nothing else, touching no application state.
///
/// The function bodies below are stringified, so they may only use what they define plus the
/// browser globals — no closure over this module. That is enforced by the fact that they are
/// pasted, not called, and by the round-trip self-test in the suite.
function pageScript() {
  const math = [parseCssColor, srgbToLinear, relativeLuminance, compositeOver, contrastRatio, round2, isLargeText, hex]
    .map(function (f) { return f.toString(); })
    .join("\n\n");
  return (
    "(function(){\n" + math + "\n\n" + probeBody.toString() + "\n" +
    "window.__contrastProbe = probeBody;\n" +
    // Exposed so a check can prove the arithmetic RUNNING IN WEBKIT is the arithmetic the
    // node-side check verified against the published calculator, rather than a second copy
    // that happens to agree today.
    "window.__contrastMath = { parseCssColor: parseCssColor, relativeLuminance: relativeLuminance, " +
    "contrastRatio: contrastRatio, compositeOver: compositeOver, round2: round2, isLargeText: isLargeText, hex: hex };\n" +
    "})();"
  );
}

/// THE WALK. Every visible run of text on the surface, every declared indicator, resolved
/// against the colours actually painted behind it.
///
/// The background is resolved from `document.elementsFromPoint` at a point inside the text's
/// own line box — the browser's real paint stack — and NOT from an ancestor walk. The
/// difference is the whole reason to bother: an ancestor walk cannot see the scrim an
/// overlay paints over the rail, so it happily reports the rail's own comfortable ratio for
/// text a human is looking at through a 40%-black sheet. Elements ABOVE the node in that
/// stack that are not its own descendants make it unresolvable, not passing.
///
/// A node whose text box is outside the viewport cannot be hit-tested at all. Those fall
/// back to the ancestor walk and are COUNTED SEPARATELY (`ancestorResolved`), because that
/// is a weaker resolution and the report should say how much of it there was.
function probeBody(options) {
  options = options || {};
  const NORMAL = 4.5;
  const LARGE = 3.0;
  const INDICATOR = 3.0;
  const MIN_REASON = 12;

  const SKIP = { SCRIPT: 1, STYLE: 1, LINK: 1, META: 1, TITLE: 1, HEAD: 1, NOSCRIPT: 1, BR: 1, WBR: 1, TEMPLATE: 1 };

  const out = {
    surface: options.surface || "(unnamed)",
    theme: options.theme || "(unnamed)",
    nodesConsidered: 0,
    nodesChecked: 0,
    nodesPassed: 0,
    invisible: 0,
    obscured: {},
    veiled: 0,
    ancestorResolved: 0,
    // COUNTED IN TWO BUCKETS, because they are two different facts.
    //
    // The home screen (`#home`) is a canvas surface BY DESIGN — it is `round-11.1/v1`
    // "Constellation", 7,500 objects on WebGL, and the CEO chose it. Nothing in this walk can
    // read a pixel it paints, and that is not news: `tests/home.js` measures every element on
    // it FROM THE PIXELS instead, which is the only method that can.
    //
    // So a canvas inside `#home` is MEASURED SOMEWHERE ELSE, and a canvas anywhere else is
    // UNMEASURED BY ANYTHING. Collapsing the two into one number would either turn this gate
    // red forever over a surface that is covered, or — much worse — make somebody raise the
    // threshold and lose the signal for the next canvas nobody measures.
    // A THIRD BUCKET SINCE THE PAIRING SCREEN, on exactly the same footing as the second and
    // for exactly the same reason: those two canvases ARE measured from the pixels, just not
    // here. `tests/phone.js` reads them back with `getImageData` in this same WebKit and
    // asserts the only two colors present are the `#000000` modules and the `#ffffff` quiet
    // zone this shell paints — 21:1 — and that the quiet zone really is white on all four
    // edges, which is the property that makes a code scan on a dark page at all.
    //
    // They are counted separately rather than folded into `canvasCount` for the reason the
    // note above gives about `#home`: one number would either sit red forever over something
    // that is covered, or get its threshold raised, and the next canvas nobody measures would
    // go with it.
    // A FOURTH BUCKET, the speckled ground (CEO, 2026-09-29), excluded BY NAME for the same
    // reason as the two above: it is measured, just not here. It is one decorative canvas
    // (`aria-hidden`, `pointer-events: none`) BEHIND the shell, carrying no text of its own,
    // and `tests/speckled-ground.js` walks every text node drawn over it and proves each one keeps
    // 4.5:1 against the strongest point the engine can paint under it. Because it is
    // `pointer-events: none` it never enters `elementsFromPoint`, so this walk still resolves
    // those nodes against the ruled ground on `html` — the unlit half of the same proof.
    // A FIFTH BUCKET, the Mac resources square and its card (round 47), excluded BY NAME for
    // the same reason: `tests/resources.js` reads both canvases back with `getImageData` and
    // holds every paint to 3:1 against the ground it is drawn on, in both themes.
    canvasCount: document.querySelectorAll(
      "canvas:not(#home canvas):not(#phone-sheet canvas):not(body > canvas.speckle)" +
        ":not(#res-square canvas):not(#res-card canvas)"
    ).length,
    canvasInHome: document.querySelectorAll("#home canvas").length,
    canvasInPhone: document.querySelectorAll("#phone-sheet canvas").length,
    canvasSpeckle: document.querySelectorAll("body > canvas.speckle").length,
    canvasResources: document.querySelectorAll("#res-square canvas, #res-card canvas").length,
    svgTextCount: document.querySelectorAll("svg text, svg tspan").length,
    failures: {},
    unresolvable: {},
    exempt: {},
    exemptUnneeded: {},
    indicators: { considered: 0, checked: 0, passed: 0, textOnly: 0 },
    /// Every node this walk actually resolved and measured, pass or fail. Its only consumer
    /// is the cross-surface check that proves the `obscured` bucket is not a hiding place:
    /// a node filed obscured behind a modal has to turn up MEASURED on a surface where that
    /// modal is closed, or the gate is saying nothing about it anywhere.
    measuredPaths: [],
  };

  // Painted layers the hit test cannot see. `pointer-events: none` removes an element from
  // `elementsFromPoint` while leaving it fully painted — the opening curtain is exactly this
  // — so a node underneath one of them is reported unresolvable rather than measured against
  // a background that is not what the eye receives.
  const ghosts = [];
  const everything = document.querySelectorAll("*");
  for (let i = 0; i < everything.length; i++) {
    const e = everything[i];
    if (SKIP[e.tagName]) continue;
    const s = getComputedStyle(e);
    if (s.pointerEvents !== "none" || s.display === "none" || s.visibility !== "visible") continue;
    if (cumulativeOpacity(e) <= 0.001) continue;
    const bgc = parseCssColor(s.backgroundColor) || { a: 0 };
    const painted = s.backgroundImage !== "none" || bgc.a > 0;
    if (!painted) continue;
    const r = e.getBoundingClientRect();
    if (r.width < 1 || r.height < 1) continue;
    // An OPAQUE ghost hides what is under it — that text is not being read and belongs in
    // `obscured` alongside everything behind a modal. Anything less does not hide it and
    // cannot be reasoned through either, so it stays UNRESOLVABLE.
    //
    // Opacity is judged on the background COLOUR alone, never on the presence of a
    // background-image. The first version of this line read `backgroundImage !== "none" ||
    // ...`, and the opening screen's grain — a fractal-noise tile at a few percent opacity
    // under `mix-blend-mode: overlay` — counted as an opaque occluder. Its tagline, the one
    // line of HTML text on that whole screen, was filed as "not being read" and the surface
    // walked 0 nodes while reporting nothing wrong. An image whose pixels cannot be read is
    // a reason to refuse, not a reason to assume it covers everything.
    const opaque = bgc.a * cumulativeOpacity(e) >= 0.5;
    ghosts.push({ el: e, rect: r, opaque: opaque });
  }

  function cumulativeOpacity(el) {
    let o = 1;
    let n = el;
    while (n && n.nodeType === 1) {
      const v = parseFloat(getComputedStyle(n).opacity);
      if (isFinite(v)) o *= v;
      n = n.parentElement;
    }
    return o;
  }

  function shortPath(el) {
    const bit = function (e) {
      let s = e.tagName.toLowerCase();
      if (e.id) return s + "#" + e.id;
      const cls = (e.getAttribute("class") || "").trim().split(/\s+/).filter(Boolean).slice(0, 3);
      if (cls.length) s += "." + cls.join(".");
      return s;
    };
    const parts = [];
    let n = el;
    for (let d = 0; n && n.nodeType === 1 && d < 4; d++) {
      parts.unshift(bit(n));
      if (n.id) break;
      n = n.parentElement;
    }
    return parts.join(" > ");
  }

  /// The style signature a failure is filed under. DELIBERATELY not the node: contrast is a
  /// property of a colour PAIRING at a size, and the same pairing on forty rail rows is one
  /// defect, not forty. Keying on the node would also make every entry churn the moment the
  /// fixture's text changed.
  function signature(el, cs, fg, bg, threshold) {
    const cls = (el.getAttribute("class") || "").trim().split(/\s+/).filter(Boolean).slice(0, 3).join(".");
    const sel = el.tagName.toLowerCase() + (cls ? "." + cls : "");
    return sel + " | " + hex(fg) + " on " + hex(bg) + " | " + Math.round(parseFloat(cs.fontSize) * 10) / 10 + "px/" + cs.fontWeight + " | >=" + threshold;
  }

  function file(bucket, key, entry) {
    if (!bucket[key]) {
      bucket[key] = entry;
      bucket[key].nodes = 1;
    } else {
      bucket[key].nodes++;
    }
  }

  /// Composite the paint stack from the bottom up. Returns either a resolved opaque colour
  /// or the REASON it could not be resolved — never a guess.
  function resolveBackground(chain) {
    const layers = [];
    for (let i = 0; i < chain.length; i++) {
      const e = chain[i];
      const s = getComputedStyle(e);
      const where = " on " + shortPath(e);
      if (s.backgroundImage && s.backgroundImage !== "none") return { why: "background-image or gradient" + where };
      if (s.mixBlendMode && s.mixBlendMode !== "normal") return { why: "mix-blend-mode: " + s.mixBlendMode + where };
      if (s.filter && s.filter !== "none") return { why: "filter: " + s.filter + where };
      if (s.backdropFilter && s.backdropFilter !== "none") return { why: "backdrop-filter" + where };
      // KEPT, AND KNOWN TO BE INERT ON THIS ENGINE. WebKit's getComputedStyle reports
      // `border-box` for `background-clip: text`, prefixed or not, so this guard never fires
      // under the engine Tauri ships and cannot be relied on. The gradient-text idiom is
      // caught by its other half instead: `background-clip: text` is useless without
      // `color: transparent`, and transparent text composites to exactly its own background
      // for a ratio of 1.0 — the walk fails it as unreadable, which it is. Check 7 asserts
      // that, rather than asserting this line.
      const clip = s.webkitBackgroundClip || s.backgroundClip;
      if (clip === "text") return { why: "background-clip: text" + where };
      const c = parseCssColor(s.backgroundColor);
      if (!c) return { why: "unparseable background-color '" + s.backgroundColor + "'" + where };
      const a = c.a * cumulativeOpacity(e);
      if (a > 0.001) layers.push({ r: c.r, g: c.g, b: c.b, a: a });
      if (a >= 0.999) {
        // An opaque backstop. Composite downward from it and stop.
        let acc = { r: layers[layers.length - 1].r, g: layers[layers.length - 1].g, b: layers[layers.length - 1].b, a: 1 };
        for (let k = layers.length - 2; k >= 0; k--) acc = compositeOver(layers[k], acc);
        return { color: acc };
      }
    }
    return { why: "no opaque layer anywhere in the paint stack — the page has nothing to sit on" };
  }

  /// THE SHEET OVER THE TEXT IS PART OF THE ANSWER. An earlier version of this function
  /// returned "painted over by <the overlay>" and filed the node unresolvable, which turned
  /// every one of the rail's thirty rows into a fresh unresolvable line the moment a modal
  /// opened — thirty lines of noise per surface, about nodes this same suite measures
  /// properly on the surface where they are the subject. Noise is how a checker gets muted.
  ///
  /// So the layers above are COLLECTED, not complained about, and the caller decides:
  ///
  ///   * a real scrim (>= 50% effective alpha) means the CEO is not reading that text right
  ///     now — filed `obscured`, and a check asserts every obscured node IS measured on some
  ///     other surface, so the bucket cannot become a hiding place;
  ///   * a faint veil (< 50%) excuses nothing — it is composited over BOTH the text and its
  ///     background and the ratio is measured THROUGH it, which is what the eye receives;
  ///   * a layer whose colour cannot be reasoned about at all is still unresolvable.
  function stackFor(el, pt) {
    if (pt) {
      const stack = document.elementsFromPoint(pt.x, pt.y);
      const at = stack.indexOf(el);
      if (at >= 0) {
        const over = [];
        for (let i = at - 1; i >= 0; i--) {
          if (el.contains(stack[i])) continue;
          const s = getComputedStyle(stack[i]);
          if (s.backgroundImage !== "none") return { why: "an opaque-unknown layer over it: background-image on " + shortPath(stack[i]) };
          if (s.mixBlendMode !== "normal" || s.filter !== "none") return { why: "a blended or filtered layer over it: " + shortPath(stack[i]) };
          const c = parseCssColor(s.backgroundColor);
          if (!c) return { why: "unparseable background-color on the layer over it: " + shortPath(stack[i]) };
          const a = c.a * cumulativeOpacity(stack[i]);
          // A MODAL'S OWN SCRIM, identified by the app's OWN semantics rather than by a
          // guess about alpha. `aria-modal="true"` is a statement that everything outside
          // this dialog is inert, and inert text is not text the CEO is being asked to read.
          // Keying on the attribute rather than on "the sheet is dark enough" is what stops
          // this from being a knob: a 2%-alpha sheet cannot excuse anything unless the app
          // is also claiming, to every screen reader, that the page behind it is inert.
          const modal = stack[i].closest('[aria-modal="true"], [role="dialog"]');
          if (modal && !modal.contains(el)) return { modalScrim: shortPath(modal) };
          if (a > 0.001) over.push({ r: c.r, g: c.g, b: c.b, a: a, by: shortPath(stack[i]) });
        }
        return { chain: stack.slice(at), over: over };
      }
    }
    // Not hit-testable (pointer-events) or off-viewport: the weaker resolution, counted.
    const chain = [];
    let n = el;
    while (n && n.nodeType === 1) {
      chain.push(n);
      n = n.parentElement;
    }
    return { chain: chain, viaAncestors: true };
  }

  function ghostOver(el, pt) {
    if (!pt) return null;
    for (let i = 0; i < ghosts.length; i++) {
      const g = ghosts[i];
      if (g.el === el || g.el.contains(el) || el.contains(g.el)) continue;
      if (pt.x >= g.rect.left && pt.x <= g.rect.right && pt.y >= g.rect.top && pt.y <= g.rect.bottom) {
        return { path: shortPath(g.el), opaque: g.opaque };
      }
    }
    return null;
  }

  /// An exemption is a CLAIM that this text is not meant to be read closely. It is honoured,
  /// and it is printed. An empty or one-word reason is not a claim, it is a mute button, and
  /// is rejected as a failure of its own.
  function exemptionFor(el) {
    const holder = el.closest("[data-contrast-exempt]");
    if (!holder) return null;
    const reason = (holder.getAttribute("data-contrast-exempt") || "").trim();
    return { holder: holder, reason: reason, valid: reason.length >= MIN_REASON };
  }

  function thresholdFor(el, cs) {
    if (el.closest("[data-contrast-role='indicator']")) return INDICATOR;
    return isLargeText(parseFloat(cs.fontSize), cs.fontWeight) ? LARGE : NORMAL;
  }

  // ---- text ---------------------------------------------------------------------------

  for (let i = 0; i < everything.length; i++) {
    const el = everything[i];
    if (SKIP[el.tagName]) continue;
    if (el.ownerSVGElement || el.tagName === "svg") continue; // named as uncovered, not skipped quietly

    let text = "";
    for (let k = 0; k < el.childNodes.length; k++) {
      if (el.childNodes[k].nodeType === 3) text += el.childNodes[k].nodeValue;
    }
    if (!text.trim()) continue;
    out.nodesConsidered++;

    const cs = getComputedStyle(el);
    if (cs.display === "none" || cs.visibility !== "visible") { out.invisible++; continue; }

    const range = document.createRange();
    range.selectNodeContents(el);
    const rects = range.getClientRects();
    let rect = null;
    for (let k = 0; k < rects.length; k++) {
      if (rects[k].width >= 1 && rects[k].height >= 1) { rect = rects[k]; break; }
    }
    range.detach && range.detach();
    if (!rect) { out.invisible++; continue; }

    const opacity = cumulativeOpacity(el);
    if (opacity <= 0.001) { out.invisible++; continue; }

    const px = rect.left + rect.width / 2;
    const py = rect.top + rect.height / 2;
    const inView = px >= 0 && py >= 0 && px < window.innerWidth && py < window.innerHeight;
    const pt = inView ? { x: px, y: py } : null;

    const snippet = text.replace(/\s+/g, " ").trim().slice(0, 48);
    const exemption = exemptionFor(el);

    const ghost = ghostOver(el, pt);
    if (ghost && ghost.opaque) {
      file(out.obscured, shortPath(el), { path: shortPath(el), text: snippet, by: ghost.path, covered: "under an opaque non-hit-testable layer" });
      continue;
    }
    const st = ghost ? { why: "covered by a translucent, non-hit-testable layer: " + ghost.path } : stackFor(el, pt);

    if (st.modalScrim) {
      file(out.obscured, shortPath(el), { path: shortPath(el), text: snippet, by: st.modalScrim, covered: "behind an aria-modal dialog" });
      continue;
    }

    let why = st.why || null;
    let bg = null;
    let veil = null;
    if (!why) {
      const over = st.over || [];
      // 1 - PI(1-a): what is left of the text after every sheet above it.
      let clear = 1;
      for (let k = 0; k < over.length; k++) clear *= 1 - over[k].a;
      const covered = 1 - clear;
      if (covered >= 0.5) {
        // Behind a real scrim. Not read, so not measured here — and named, so the check
        // downstream can prove it was measured somewhere it IS read.
        file(out.obscured, shortPath(el), { path: shortPath(el), text: snippet, by: over.map(function (o) { return o.by; }).join(", "), covered: round2(covered) });
        continue;
      }
      if (covered > 0.001) { veil = over; out.veiled++; }
      if (st.viaAncestors) out.ancestorResolved++;
      const r = resolveBackground(st.chain);
      if (r.why) why = r.why;
      else bg = r.color;
    }

    /// A faint sheet lands on the text and on its background alike.
    const through = function (c) {
      let acc = c;
      if (veil) for (let k = 0; k < veil.length; k++) acc = compositeOver(veil[k], acc);
      return acc;
    };

    if (why) {
      // FAILURE TO PROVE. Not a pass, not a skip. Exempt text is allowed to be unprovable —
      // that is what the exemption says — but it is still printed.
      if (exemption && exemption.valid) {
        file(out.exempt, exemption.reason + " :: " + shortPath(el), {
          reason: exemption.reason, path: shortPath(el), text: snippet, verdict: "unresolvable: " + why,
        });
        continue;
      }
      file(out.unresolvable, why + " :: " + shortPath(el), { why: why, path: shortPath(el), text: snippet });
      continue;
    }

    const rawFg = parseCssColor(cs.color);
    if (!rawFg) {
      file(out.unresolvable, "unparseable color '" + cs.color + "' :: " + shortPath(el), {
        why: "unparseable color '" + cs.color + "'", path: shortPath(el), text: snippet,
      });
      continue;
    }
    bg = through(bg);
    const fg = through(compositeOver({ r: rawFg.r, g: rawFg.g, b: rawFg.b, a: rawFg.a * opacity }, bg));
    const threshold = thresholdFor(el, cs);
    const ratio = round2(contrastRatio(fg, bg));
    const ok = ratio >= threshold;

    out.nodesChecked++;
    out.measuredPaths.push(shortPath(el));

    if (exemption) {
      if (!exemption.valid) {
        file(out.unresolvable, "empty or one-word data-contrast-exempt :: " + shortPath(exemption.holder), {
          why: "data-contrast-exempt with no stated reason ('" + exemption.reason + "') — an exemption must say what it is claiming",
          path: shortPath(exemption.holder), text: snippet,
        });
        continue;
      }
      const key = exemption.reason + " :: " + signature(el, cs, fg, bg, threshold);
      if (ok) {
        file(out.exemptUnneeded, key, { reason: exemption.reason, ratio: ratio, threshold: threshold, path: shortPath(el), text: snippet });
        out.nodesPassed++;
      } else {
        file(out.exempt, key, { reason: exemption.reason, ratio: ratio, threshold: threshold, path: shortPath(el), text: snippet, verdict: ratio + ":1 against a floor of " + threshold + ":1" });
      }
      continue;
    }

    if (ok) { out.nodesPassed++; continue; }

    file(out.failures, signature(el, cs, fg, bg, threshold), {
      selector: shortPath(el),
      ratio: ratio,
      threshold: threshold,
      fg: hex(fg),
      bg: hex(bg),
      fontSize: Math.round(parseFloat(cs.fontSize) * 10) / 10,
      fontWeight: cs.fontWeight,
      text: snippet,
      large: isLargeText(parseFloat(cs.fontSize), cs.fontWeight),
    });
  }

  // ---- text the STYLESHEET puts on screen (`::before` / `::after`) -----------------------
  //
  // THE WALK ABOVE COLLECTS `nodeType === 3` CHILDREN, AND A PSEUDO-ELEMENT IS NOT A NODE.
  // So every character rendered by a `content:` declaration was outside this file entirely —
  // not measured, not counted, and not on the "WHAT IT DOES NOT SEE" list at the top, which
  // is the part that made it dangerous. A surface could print `N nodes measured, 0 failures`
  // with authored text on it that nothing had ever looked at.
  //
  // There is one in the shipped tree, and it is not a decoration: `style.css`'s
  // `.setbtn::after { content: "Settings" }` — the tooltip on the settings button §15 puts
  // on every screen, including the opening screen where it is the only control there is.
  // `lib/ui-sources.js`'s `cssContentStrings()` derives the same inventory from the
  // STYLESHEETS, and `contrast.js` joins the two so neither side can be short.
  //
  // THREE BUCKETS, BECAUSE THEY ARE THREE DIFFERENT FACTS, and the middle one is the whole
  // reason this is not just "measure it":
  //
  //   checked     visible, resolvable, held to the same 4.5 / 3.0 floors as any other text.
  //   hidden      NOT ON SCREEN RIGHT NOW — `opacity: 0` until hover, `display: none`.
  //               Named rather than skipped, so a caller can drive it into view and come
  //               back; the settings tooltip is exactly this and is measured on hover.
  //   unprovable  visible, and its ink or its background could not be resolved. A FAILURE
  //               TO PROVE, never a pass — the same rule the rest of this file uses.
  //
  // WHY AN ABSOLUTELY-POSITIONED PSEUDO WITH A TRANSPARENT BACKGROUND IS UNPROVABLE RATHER
  // THAN MEASURED AGAINST ITS HOST. A pseudo-element has no `Range` and no `getClientRects`,
  // so there is no point to hit-test with. For one that flows INSIDE its host that does not
  // matter — its background is the host's. For one that is `position: absolute` it does: the
  // settings tooltip sits `calc(100% + 8px)` BELOW the button, over whatever the app is
  // showing there, and compositing it over the button's own background would report a
  // comfortable ratio for a stack that is not the one the eye receives. That is precisely
  // the ancestor-walk mistake this file removed from `outage.js` on 2026-09-05.
  out.generated = { considered: 0, checked: 0, passed: 0, hidden: {}, unprovable: {}, measured: [] };
  for (let i = 0; i < everything.length; i++) {
    const el = everything[i];
    if (SKIP[el.tagName]) continue;
    if (el.ownerSVGElement || el.tagName === "svg") continue;
    for (let p = 0; p < 2; p++) {
      const pseudo = p === 0 ? "::before" : "::after";
      const gs = getComputedStyle(el, pseudo);
      const raw = gs.content;
      if (!raw || raw === "none" || raw === "normal") continue;

      // The authored text: every quoted run in the `content` list, concatenated. `attr()`,
      // `counter()` and `url()` contribute nothing here — they are not authored sentences,
      // and there are none in this tree.
      let text = "";
      const q = /"((?:\\.|[^"\\])*)"|'((?:\\.|[^'\\])*)'/g;
      let qm;
      while ((qm = q.exec(raw))) text += qm[1] !== undefined ? qm[1] : qm[2];
      if (!text.trim()) continue; // `content: ""` — a decorative box, not text
      out.generated.considered++;

      const path = shortPath(el) + pseudo;
      const snippet = text.replace(/\s+/g, " ").trim().slice(0, 48);
      const opacity = cumulativeOpacity(el) * (parseFloat(gs.opacity) || 0);

      if (gs.display === "none" || gs.visibility !== "visible" || !(opacity > 0.001)) {
        file(out.generated.hidden, path, {
          path: path,
          text: snippet,
          why:
            gs.display === "none"
              ? "display: none"
              : gs.visibility !== "visible"
              ? "visibility: " + gs.visibility
              : "cumulative opacity " + round2(opacity),
        });
        continue;
      }

      const rawFg = parseCssColor(gs.color);
      if (!rawFg) {
        file(out.generated.unprovable, path, { path: path, text: snippet, why: "unparseable color '" + gs.color + "'" });
        continue;
      }
      if (gs.backgroundImage !== "none") {
        file(out.generated.unprovable, path, { path: path, text: snippet, why: "a background-image behind generated text" });
        continue;
      }
      const own = parseCssColor(gs.backgroundColor);
      let bg = null;
      let via = "its own background";
      if (own && own.a >= 0.999) {
        bg = own;
      } else if (gs.position === "absolute" || gs.position === "fixed") {
        file(out.generated.unprovable, path, {
          path: path,
          text: snippet,
          why:
            "positioned " + gs.position + " with a background that is not opaque — it can paint " +
            "anywhere, and a pseudo-element has no box this walk can hit-test",
        });
        continue;
      } else {
        // It flows inside its host, so the host's resolved stack IS its background.
        const r = el.getBoundingClientRect();
        const pt =
          r.width >= 1 && r.height >= 1 && r.left >= 0 && r.top >= 0 &&
          r.left < window.innerWidth && r.top < window.innerHeight
            ? { x: r.left + r.width / 2, y: r.top + r.height / 2 }
            : null;
        const st = stackFor(el, pt);
        if (st.why) {
          file(out.generated.unprovable, path, { path: path, text: snippet, why: st.why });
          continue;
        }
        const rb = resolveBackground(st.chain);
        if (rb.why) {
          file(out.generated.unprovable, path, { path: path, text: snippet, why: rb.why });
          continue;
        }
        bg = own && own.a > 0 ? compositeOver(own, rb.color) : rb.color;
        via = "its host's resolved stack";
      }

      const fg = compositeOver({ r: rawFg.r, g: rawFg.g, b: rawFg.b, a: rawFg.a * opacity }, bg);
      const size = parseFloat(gs.fontSize);
      const threshold = el.closest("[data-contrast-role='indicator']")
        ? INDICATOR
        : isLargeText(size, gs.fontWeight)
        ? LARGE
        : NORMAL;
      const ratio = round2(contrastRatio(fg, bg));
      out.generated.checked++;
      out.generated.measured.push({
        path: path,
        text: snippet,
        ratio: ratio,
        threshold: threshold,
        fg: hex(fg),
        bg: hex(bg),
        fontSize: Math.round(size * 10) / 10,
        fontWeight: gs.fontWeight,
        via: via,
      });
      if (ratio >= threshold) {
        out.generated.passed++;
        continue;
      }
      file(out.failures, "generated " + path + " " + hex(fg) + " on " + hex(bg), {
        selector: path,
        ratio: ratio,
        threshold: threshold,
        fg: hex(fg),
        bg: hex(bg),
        fontSize: Math.round(size * 10) / 10,
        fontWeight: gs.fontWeight,
        text: snippet,
        large: isLargeText(size, gs.fontWeight),
      });
    }
  }

  // ---- non-text indicators (§1.4.11) ----------------------------------------------------
  //
  // Bounded ON PURPOSE, and the bound is stated: a control is "distinguishable" if either its
  // own fill or its border reaches 3:1 against what surrounds it. A control that is neither
  // filled nor bordered has no non-text indicator to check — its TEXT is the affordance, and
  // the walk above already covered it at 4.5:1. Those are counted as `textOnly` rather than
  // waved through, because that count is the honest size of this section's blind spot.

  const CONTROLS = "button, input, textarea, select, [role='button'], [role='textbox'], [role='listbox'], [role='option'], [data-contrast-role='indicator']";
  const controls = document.querySelectorAll(CONTROLS);
  for (let i = 0; i < controls.length; i++) {
    const el = controls[i];
    const cs = getComputedStyle(el);
    if (cs.display === "none" || cs.visibility !== "visible") continue;
    const rect = el.getBoundingClientRect();
    if (rect.width < 1 || rect.height < 1) continue;
    if (cumulativeOpacity(el) <= 0.001) continue;
    out.indicators.considered++;

    // A CONTROL WITH A VISIBLE LABEL IS IDENTIFIED BY ITS LABEL. §1.4.11 is about the visual
    // information REQUIRED to identify a component; when a button says "Send", the word is
    // that information and the walk above already held it to 4.5:1. Demanding 3:1 of a
    // deliberately faint tint behind a label that already passes is how a checker earns its
    // way into someone's mute list. Form fields are the exception and are always checked:
    // an empty input's boundary IS its affordance, and its value text is not a label.
    const FIELD = el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.tagName === "SELECT";
    if (!FIELD && (el.textContent || "").trim()) { out.indicators.textOnly++; continue; }

    const exemption = exemptionFor(el);

    const fill = parseCssColor(cs.backgroundColor) || { a: 0 };
    const sides = ["borderTopColor", "borderRightColor", "borderBottomColor", "borderLeftColor"];
    const widths = ["borderTopWidth", "borderRightWidth", "borderBottomWidth", "borderLeftWidth"];
    let borderColor = null;
    for (let k = 0; k < 4; k++) {
      if (parseFloat(cs[widths[k]]) >= 1) {
        const c = parseCssColor(cs[sides[k]]);
        if (c && c.a > 0.05) { borderColor = c; break; }
      }
    }
    if (fill.a <= 0.05 && !borderColor) { out.indicators.textOnly++; continue; }

    // WHAT SURROUNDS IT is the paint stack directly BEHIND it — the parent background its
    // border is drawn over — and WHETHER IT IS COVERED is a separate question, answered at
    // the control's own centre by the same `stackFor` the text walk uses.
    //
    // An earlier version conflated the two. It sampled two pixels outside the control's left
    // edge, took the whole hit stack there, and treated every element in it as a layer
    // painted OVER the control. At a point outside the control, that stack is mostly the
    // page — so `main#stage`, the paper the entire app sits on, was read as an opaque sheet
    // covering the search field, and the field was filed "not being looked at" on the one
    // surface where it is the thing the CEO is typing into. Check 11 is what caught it,
    // which is the whole reason check 11 exists: a bucket that means "measured elsewhere"
    // has to be made to prove the elsewhere.
    //
    // The curtain check stays separate above, because `pointer-events: none` keeps the
    // opening screen out of every hit test while it is painted over everything.
    const curtain = ghostOver(el, { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 });
    if (curtain && curtain.opaque) {
      file(out.obscured, shortPath(el) + " (indicator)", { path: shortPath(el) + " (indicator)", text: "(indicator)", by: curtain.path, covered: "under an opaque non-hit-testable layer" });
      continue;
    }

    const st = stackFor(el, { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 });
    if (st.modalScrim) {
      file(out.obscured, shortPath(el) + " (indicator)", { path: shortPath(el) + " (indicator)", text: "(indicator)", by: st.modalScrim, covered: "behind an aria-modal dialog" });
      continue;
    }
    if (st.why) {
      if (exemption && exemption.valid) {
        file(out.exempt, exemption.reason + " :: indicator " + shortPath(el), { reason: exemption.reason, path: shortPath(el), text: "(indicator)", verdict: "unresolvable: " + st.why });
        continue;
      }
      file(out.unresolvable, "indicator: " + st.why + " :: " + shortPath(el), { why: "indicator: " + st.why, path: shortPath(el), text: "(indicator)" });
      continue;
    }
    let sheet = 1;
    for (let k = 0; k < (st.over || []).length; k++) sheet *= 1 - st.over[k].a;
    if (1 - sheet >= 0.5) {
      file(out.obscured, shortPath(el) + " (indicator)", { path: shortPath(el) + " (indicator)", text: "(indicator)", by: st.over.map(function (o) { return o.by; }).join(", "), covered: round2(1 - sheet) });
      continue;
    }
    const at = st.chain.indexOf(el);
    const chain = at >= 0 ? st.chain.slice(at + 1) : st.chain.filter(function (e) { return e !== el; });
    if (!chain.length) { out.indicators.textOnly++; continue; }
    const around = resolveBackground(chain);
    if (around.why) {
      if (exemption && exemption.valid) {
        file(out.exempt, exemption.reason + " :: indicator " + shortPath(el), { reason: exemption.reason, path: shortPath(el), text: "(indicator)", verdict: "unresolvable: " + around.why });
        continue;
      }
      file(out.unresolvable, "indicator surround: " + around.why + " :: " + shortPath(el), { why: "indicator surround: " + around.why, path: shortPath(el), text: "(indicator)" });
      continue;
    }

    let best = 0;
    if (fill.a > 0.05) best = Math.max(best, round2(contrastRatio(compositeOver({ r: fill.r, g: fill.g, b: fill.b, a: fill.a * cumulativeOpacity(el) }, around.color), around.color)));
    if (borderColor) best = Math.max(best, round2(contrastRatio(compositeOver({ r: borderColor.r, g: borderColor.g, b: borderColor.b, a: borderColor.a * cumulativeOpacity(el) }, around.color), around.color)));

    out.indicators.checked++;
    out.measuredPaths.push(shortPath(el) + " (indicator)");
    const key = shortPath(el) + " | indicator vs " + hex(around.color) + " | >=" + INDICATOR;
    if (best >= INDICATOR) {
      out.indicators.passed++;
      if (exemption && exemption.valid) file(out.exemptUnneeded, exemption.reason + " :: " + key, { reason: exemption.reason, ratio: best, threshold: INDICATOR, path: shortPath(el), text: "(indicator)" });
      continue;
    }
    if (exemption && exemption.valid) {
      file(out.exempt, exemption.reason + " :: " + key, { reason: exemption.reason, ratio: best, threshold: INDICATOR, path: shortPath(el), text: "(indicator)", verdict: best + ":1 against a floor of " + INDICATOR + ":1" });
      continue;
    }
    file(out.failures, key, {
      selector: shortPath(el), ratio: best, threshold: INDICATOR, fg: "(fill/border)", bg: hex(around.color),
      fontSize: 0, fontWeight: "-", text: "(non-text indicator)", large: false, indicator: true,
    });
  }

  return out;
}

module.exports = {
  parseCssColor,
  srgbToLinear,
  relativeLuminance,
  compositeOver,
  contrastRatio,
  round2,
  isLargeText,
  hex,
  measureIndicatorCrop,
  pageScript,
};

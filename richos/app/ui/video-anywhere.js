// MAKE A VIDEO, ANYWHERE IN THE APP (round 79; CEO 2026-11-15: "Get someone on Fable to build
// round 79.").
//
// His words for the feature: "When the user is not on the home page (and not on the splash
// screen), we need to add a "Make a video" button in the area shown here ... unlike on the home
// page, in this case there won't be an option for when recording stops. Because in that case, the
// user will always have to stop the recording. So, the idea here is: We let the user start a screen
// recording anywhere in the app. This will record whatever they want to record/show within the app."
//
// The design is `richos-hq/design/mockups/rounds/round-79`. THE AREA HE MARKED is the heading of
// the Settings panel, at its right end. The settings button is on every screen, so the button is
// one press away on every screen. The home screen keeps its own button (round 77,
// `home-video.js`) and this one is not there on it, nor on the opening curtain.
//
// WHAT IT IS. The home screen's recorder with one row less: the microphone off or on, the mouse
// pointer shown or hidden, "Start recording". Settings closes, a count of three, and RichOS is
// recorded while he uses it. He always stops it himself, so while it records the same button, red,
// with the time and "Stop", stands beside the settings button on every screen he goes to (and in
// the heading of Settings when he opens it). Then the sheet: the video playing, "Save video",
// "Record again", "Discard".
//
// ESCAPE DOES NOT STOP A RECORDING HERE. On the home screen it does; here he is showing the app,
// and Escape is one of the keys he shows it with (it closes sheets and panels). It still closes
// the choices and cancels the count.
//
// THE FILE IS REAL. The window is drawn on one canvas from where the page put every word, plate,
// line, icon and picture (the home screen's painter, made to walk the whole app, as the round's
// own recorder does), and that canvas is recorded with the webview's own recorder, with the
// microphone's sound added when he chose it. The recorder's own pieces (`rv-skip`) are never in
// the file. TEXT HE HIGHLIGHTS IS IN THE FILE (CEO 2026-11-15, on the round's mockup: "it was
// recording the mouse but not recording text highlights when I was highlighting the text with the
// mouse. I would expect the screen recording to record the text highlighting as well."): the
// highlight is drawn behind the words he selected, in the Mac's own highlight color, in the
// conversation and in a text box, on every line of it that shows. What the
// painter leaves out: gradients behind plates, a picture the canvas may not read (a quiet plate
// stands in), the blinking text cursor, tooltips the Mac draws, a select's small arrow and a
// playing video. "Save video" hands the file to the Mac's own save window (`home_video_save`).
//
// THE MICROPHONE, the icons, the mouse pointer drawn in the file and the save window are the home
// screen's own (`RichHomeVideo.parts`): one of each in the app.
//
// Everything on the screen is built with `textContent`.

"use strict";

window.RichVideoAnywhere = (function () {
  var P = window.RichHomeVideo && window.RichHomeVideo.parts;
  if (!P) return null;
  var make = P.make;
  var fmt = P.fmt;
  var clock = P.clock;
  var radius = P.radius;
  var plate = P.plate;
  var el = {};
  var wrap = null; // `.settings`, the settings button's own row
  var state = "idle"; // idle | choosing | count | rec | saving | sheet
  var opts = { pointer: true }; // stays as he left it
  var NS = "http://www.w3.org/2000/svg";
  /// The choices are open without Settings under them (see `openChoices`).
  var loose = false;

  var icon = function (kind) {
    return P.icon(kind, "rv-ico");
  };
  var level = function () {
    return P.level("rv-level");
  };
  var body = function () {
    return document.body;
  };
  var homeOpen = function () {
    return body().classList.contains("home-open");
  };

  // THE MICROPHONE (as on the home screen): asked for when he switches it on, held only while he
  // is choosing, counting or recording.
  var mic = P.microphone(
    function () {
      paintMic();
    },
    function () {
      return document.querySelectorAll(".rv-level");
    }
  );
  function paintMic() {
    body().classList.toggle("rv-mic-on", mic.state === "on");
    if (el.micSeg) el.micSeg.paint();
    if (el.micOn) el.micOn.querySelector("b").textContent = mic.state === "asking" ? "Asking…" : "On";
    if (el.micNote) el.micNote.hidden = mic.state !== "denied";
    mic.watch();
  }

  // ---------------------------------------------------------------------------------------------
  // THE FRAME PAINTER: the app, drawn on one canvas. The home screen's painter (`home-video.js`)
  // made to walk the whole page: plates and their edges, lines, words, icons, pictures, what a
  // field holds, the small drawn pieces of a switch or a radio (`::before`, `::after`), and every
  // canvas (the speckled ground, the home screen's picture). Anything marked `rv-skip` is the
  // recorder's own.
  // ---------------------------------------------------------------------------------------------
  var CLEAR = /^rgba\(0, 0, 0, 0\)$|^transparent$/;
  var NOT = { SCRIPT: 1, STYLE: 1, LINK: 1, TEMPLATE: 1, NOSCRIPT: 1, OPTION: 1, OPTGROUP: 1, HEAD: 1, VIDEO: 1, AUDIO: 1, IFRAME: 1 };
  var NEVER = { "home-grain": 1, "home-loading": 1 };
  var range = document.createRange();
  function shapePath(n) {
    var t = n.tagName.toLowerCase();
    var num = function (a) {
      return parseFloat(n.getAttribute(a)) || 0;
    };
    var p = new Path2D();
    if (t === "path") return n.getAttribute("d") ? new Path2D(n.getAttribute("d")) : null;
    if (t === "circle") p.arc(num("cx"), num("cy"), num("r"), 0, Math.PI * 2);
    else if (t === "ellipse") p.ellipse(num("cx"), num("cy"), num("rx"), num("ry"), 0, 0, Math.PI * 2);
    else if (t === "rect") {
      var rx = num("rx") || num("ry");
      if (rx && p.roundRect) p.roundRect(num("x"), num("y"), num("width"), num("height"), rx);
      else p.rect(num("x"), num("y"), num("width"), num("height"));
    } else if (t === "line") {
      p.moveTo(num("x1"), num("y1"));
      p.lineTo(num("x2"), num("y2"));
    } else if (t === "polyline" || t === "polygon") {
      var pts = (n.getAttribute("points") || "").trim().split(/[\s,]+/).map(Number);
      for (var i = 0; i + 1 < pts.length; i += 2) (i ? p.lineTo : p.moveTo).call(p, pts[i], pts[i + 1]);
      if (t === "polygon") p.closePath();
    } else return null;
    return p;
  }
  function paintSvg(cx, e, alpha) {
    var r = e.getBoundingClientRect();
    if (!r.width || !r.height) return;
    cx.save();
    // a drawing is cut at its own box, as the page cuts it
    cx.beginPath();
    cx.rect(r.left, r.top, r.width, r.height);
    cx.clip();
    var shapes = e.querySelectorAll("path, circle, ellipse, rect, line, polyline, polygon");
    for (var i = 0; i < shapes.length; i++) {
      var n = shapes[i];
      // a mask's or a clip's own shapes are not drawn
      if (n.closest("defs, mask, clipPath, symbol")) continue;
      var ps = getComputedStyle(n);
      if (ps.display === "none" || ps.visibility !== "visible") continue;
      var p2 = shapePath(n);
      // where the page puts this shape: its own turns and scales, its groups', the drawing's box
      var m = n.getScreenCTM ? n.getScreenCTM() : null;
      if (!p2 || !m) continue;
      var op = parseFloat(ps.opacity);
      cx.save();
      cx.transform(m.a, m.b, m.c, m.d, m.e, m.f);
      cx.globalAlpha = alpha * (isNaN(op) ? 1 : op);
      // `currentColor` is the shape's own ink, not the canvas's
      var ink = function (v) {
        return /^currentcolor$/i.test(v) ? ps.color : v;
      };
      if (ps.fill && ps.fill !== "none" && n.tagName.toLowerCase() !== "line") {
        cx.fillStyle = ink(ps.fill);
        cx.fill(p2);
      }
      if (ps.stroke && ps.stroke !== "none") {
        cx.strokeStyle = ink(ps.stroke);
        cx.lineWidth = parseFloat(ps.strokeWidth) || 1;
        cx.lineCap = ps.strokeLinecap || "butt";
        cx.lineJoin = ps.strokeLinejoin || "miter";
        cx.stroke(p2);
      }
      cx.restore();
    }
    cx.restore();
  }
  function setFont(cx, s) {
    cx.font = s.fontStyle + " " + s.fontWeight + " " + s.fontSize + " " + s.fontFamily;
  }
  // THE TEXT HE HAS HIGHLIGHTED, read once a frame: the range he selected on the page, and the
  // Mac's own highlight color (`Highlight`, read off a piece that is never shown, so it follows
  // the theme and the color he chose in System Settings).
  var tint = null;
  function selected() {
    var sel = window.getSelection ? window.getSelection() : null;
    var r = sel && sel.rangeCount && !sel.isCollapsed ? sel.getRangeAt(0) : null;
    var field = document.activeElement;
    if (!r && !(field && (field.tagName === "INPUT" || field.tagName === "TEXTAREA"))) return null;
    if (!tint) {
      tint = make("i", "rv-skip");
      tint.style.cssText = "display:none;background-color:Highlight";
      document.body.appendChild(tint);
    }
    return { range: r, color: getComputedStyle(tint).backgroundColor };
  }
  function paintText(cx, node, s, alpha, R, ink) {
    var t = node.textContent;
    var hl = R.sel && R.sel.range;
    // the highlight, behind the words: the part of this piece of text that is inside what he selected
    if (hl && s.userSelect !== "none" && s.webkitUserSelect !== "none" && hl.intersectsNode(node)) {
      var from = node === hl.startContainer ? hl.startOffset : 0;
      var to = node === hl.endContainer ? hl.endOffset : t.length;
      if (to > from) {
        range.setStart(node, from);
        range.setEnd(node, to);
        var lit = range.getClientRects();
        cx.globalAlpha = alpha;
        cx.fillStyle = R.sel.color;
        for (var q = 0; q < lit.length; q++) if (lit[q].width > 0) cx.fillRect(lit[q].left, lit[q].top, lit[q].width, lit[q].height);
      }
    }
    if (!/\S/.test(t)) return;
    setFont(cx, s);
    cx.fillStyle = ink || s.color;
    cx.globalAlpha = alpha;
    var ls = parseFloat(s.letterSpacing) || 0;
    var native = "letterSpacing" in cx;
    if (native) cx.letterSpacing = ls + "px";
    var upper = s.textTransform === "uppercase";
    var re = /\S+/g;
    var m;
    var H = innerHeight;
    /// Letters that stand on one line, drawn where the page put the first of them.
    var run = function (a, b, rc) {
      if (!rc || rc.width === 0 || rc.bottom < 0 || rc.top > H) return;
      var w = upper ? t.slice(a, b).toUpperCase() : t.slice(a, b);
      var mt = cx.measureText(w);
      var fa = mt.fontBoundingBoxAscent || parseFloat(s.fontSize) * 0.8;
      var fd = mt.fontBoundingBoxDescent || parseFloat(s.fontSize) * 0.2;
      var y = rc.top + (rc.height - (fa + fd)) / 2 + fa;
      if (native || !ls) cx.fillText(w, rc.left, y);
      else {
        var x = rc.left;
        for (var i = 0; i < w.length; i++) {
          cx.fillText(w[i], x, y);
          x += cx.measureText(w[i]).width + ls;
        }
      }
    };
    while ((m = re.exec(t))) {
      var a = m.index;
      var b = a + m[0].length;
      range.setStart(node, a);
      range.setEnd(node, b);
      var rects = range.getClientRects();
      if (!rects.length) continue;
      if (rects.length === 1) {
        run(a, b, rects[0]);
        continue;
      }
      // A WORD THE PAGE BROKE OVER LINES (a long address, text with no spaces): each line's part
      // of it is drawn on its own line, found letter by letter.
      var at = a;
      var first = null;
      for (var c = a; c <= b; c++) {
        var cr = null;
        if (c < b) {
          range.setStart(node, c);
          range.setEnd(node, c + 1);
          cr = range.getClientRects()[0];
          if (!cr) continue;
        }
        if (first && (c === b || Math.abs(cr.top - first.top) > 1)) {
          run(at, c, first);
          first = null;
        }
        if (!first && cr) {
          first = cr;
          at = c;
        }
      }
    }
    if (native) cx.letterSpacing = "0px";
  }
  // WHAT A BOX OF SEVERAL LINES HOLDS (the message box, Your instructions): every line of it that
  // shows, wrapped and scrolled as the box wraps and scrolls it, with what he highlighted in it.
  // The words of a text box cannot be asked where they stand, so the same words are laid out in a
  // piece of the page of the same width and face that is never seen, standing exactly where the
  // box's own words stand, and that piece is drawn as any other text is.
  var twin = null;
  var COPIED = ["fontStyle", "fontWeight", "fontSize", "fontFamily", "lineHeight", "letterSpacing", "wordSpacing", "textAlign", "textIndent", "textTransform", "tabSize", "paddingTop", "paddingRight", "paddingBottom", "paddingLeft", "borderTopWidth", "borderRightWidth", "borderBottomWidth", "borderLeftWidth"];
  function paintArea(cx, e, s, r, alpha, R) {
    var text = e.value;
    var faint = "";
    if (!text) {
      text = e.getAttribute("placeholder") || "";
      faint = getComputedStyle(e, "::placeholder").color;
      if (CLEAR.test(faint)) return; // a box whose own placeholder the app does not paint
    }
    if (!/\S/.test(text)) return;
    if (!twin) {
      twin = make("div", "rv-skip rv-twin", "", { "aria-hidden": "true" });
      document.body.appendChild(twin);
    }
    var bl = parseFloat(s.borderLeftWidth) || 0;
    var br = parseFloat(s.borderRightWidth) || 0;
    var bt = parseFloat(s.borderTopWidth) || 0;
    var bb = parseFloat(s.borderBottomWidth) || 0;
    for (var i = 0; i < COPIED.length; i++) if (twin.style[COPIED[i]] !== s[COPIED[i]]) twin.style[COPIED[i]] = s[COPIED[i]];
    twin.style.left = r.left + "px";
    twin.style.top = r.top - e.scrollTop + "px";
    twin.style.width = e.clientWidth + bl + br + "px";
    if (twin.textContent !== text) twin.textContent = text;
    var node = twin.firstChild;
    var sel = null;
    if (R.sel && !faint && e === document.activeElement && e.selectionEnd > e.selectionStart) {
      sel = { range: document.createRange(), color: R.sel.color };
      sel.range.setStart(node, Math.min(e.selectionStart, text.length));
      sel.range.setEnd(node, Math.min(e.selectionEnd, text.length));
    }
    cx.save();
    cx.beginPath();
    cx.rect(r.left + bl, r.top + bt, r.width - bl - br, r.height - bt - bb);
    cx.clip();
    paintText(cx, node, s, alpha, { sel: sel }, faint);
    cx.restore();
  }
  /// What a field of one line holds (or asks for), with what he highlighted in it.
  function paintField(cx, e, s, r, alpha, R) {
    var tag = e.tagName;
    if (tag === "TEXTAREA") return paintArea(cx, e, s, r, alpha, R);
    var type = (e.getAttribute("type") || "text").toLowerCase();
    var text = "";
    var faint = "";
    if (tag === "SELECT") text = e.selectedOptions && e.selectedOptions[0] ? e.selectedOptions[0].textContent : "";
    else if (tag === "TEXTAREA" || (tag === "INPUT" && /^(text|search|email|url|tel|number|password)$/.test(type))) {
      text = e.value;
      if (!text) {
        text = e.getAttribute("placeholder") || "";
        faint = getComputedStyle(e, "::placeholder").color;
        if (CLEAR.test(faint)) return; // a field whose own placeholder the app does not paint
      } else if (type === "password") text = text.replace(/./g, "•");
    } else return;
    text = text.split("\n")[0];
    if (!text.trim()) return;
    setFont(cx, s);
    var mt = cx.measureText(text);
    var fa = mt.fontBoundingBoxAscent || parseFloat(s.fontSize) * 0.8;
    var fd = mt.fontBoundingBoxDescent || parseFloat(s.fontSize) * 0.2;
    var bl = parseFloat(s.borderLeftWidth) || 0;
    var bt = parseFloat(s.borderTopWidth) || 0;
    var x = r.left + bl + (parseFloat(s.paddingLeft) || 0);
    var lh = parseFloat(s.lineHeight) || fa + fd;
    var y = tag === "TEXTAREA" ? r.top + bt + (parseFloat(s.paddingTop) || 0) + (lh - (fa + fd)) / 2 + fa : r.top + (r.height - (fa + fd)) / 2 + fa;
    cx.save();
    cx.beginPath();
    cx.rect(r.left, r.top, r.width, r.height);
    cx.clip();
    cx.globalAlpha = alpha;
    if (R.sel && !faint && e === document.activeElement && tag !== "SELECT" && type !== "password") {
      var a0 = Math.min(e.selectionStart || 0, text.length);
      var a1 = Math.min(e.selectionEnd || 0, text.length);
      if (a1 > a0) {
        cx.fillStyle = R.sel.color;
        cx.fillRect(x + cx.measureText(text.slice(0, a0)).width, y - fa, cx.measureText(text.slice(a0, a1)).width, fa + fd);
      }
    }
    cx.fillStyle = faint || s.color;
    cx.fillText(text, x, y);
    cx.restore();
  }
  /// The small drawn pieces: a switch's thumb, a radio's dot, a mark beside a row. Only the ones
  /// placed by `position: absolute` inside their own element, which is how the app draws them.
  var pseudoSeen = new WeakMap();
  function pseudoKinds(e, now) {
    var c = pseudoSeen.get(e);
    if (c && now - c.t < 400) return c.k;
    var k = [];
    ["::before", "::after"].forEach(function (which) {
      var ps = getComputedStyle(e, which);
      if (ps.content && ps.content !== "none" && ps.content !== "normal" && ps.display !== "none") k.push(which);
    });
    pseudoSeen.set(e, { t: now, k: k });
    return k;
  }
  function paintPseudo(cx, e, s, r, alpha, which) {
    var ps = getComputedStyle(e, which);
    if (ps.position !== "absolute" || s.position === "static" || ps.visibility !== "visible") return;
    var w = parseFloat(ps.width);
    var h = parseFloat(ps.height);
    if (!(w > 0) || !(h > 0)) return;
    var a = alpha * (parseFloat(ps.opacity) || 0);
    if (a < 0.012) return;
    var bw = parseFloat(ps.borderTopWidth) || 0;
    var W = w + (ps.boxSizing === "border-box" ? 0 : 2 * bw + (parseFloat(ps.paddingLeft) || 0) + (parseFloat(ps.paddingRight) || 0));
    var Hh = h + (ps.boxSizing === "border-box" ? 0 : 2 * bw + (parseFloat(ps.paddingTop) || 0) + (parseFloat(ps.paddingBottom) || 0));
    var ox = r.left + (parseFloat(s.borderLeftWidth) || 0);
    var oy = r.top + (parseFloat(s.borderTopWidth) || 0);
    var iw = r.width - (parseFloat(s.borderLeftWidth) || 0) - (parseFloat(s.borderRightWidth) || 0);
    var ih = r.height - (parseFloat(s.borderTopWidth) || 0) - (parseFloat(s.borderBottomWidth) || 0);
    var x = ps.left !== "auto" ? ox + parseFloat(ps.left) : ps.right !== "auto" ? ox + iw - parseFloat(ps.right) - W : ox;
    var y = ps.top !== "auto" ? oy + parseFloat(ps.top) : ps.bottom !== "auto" ? oy + ih - parseFloat(ps.bottom) - Hh : oy;
    x += parseFloat(ps.marginLeft) || 0;
    y += parseFloat(ps.marginTop) || 0;
    var tm = /^matrix\(([^)]+)\)$/.exec(ps.transform || "");
    var sx = 1;
    if (tm) {
      var v = tm[1].split(",").map(Number);
      // a move and an even scale about the middle; a turned piece is left as it stands
      if (Math.abs(v[1]) > 0.01 || Math.abs(v[2]) > 0.01) return;
      sx = v[0];
      x += v[4] + (W - W * v[0]) / 2;
      y += v[5] + (Hh - Hh * v[3]) / 2;
      W *= v[0];
      Hh *= v[3];
    }
    if (W <= 0 || Hh <= 0) return;
    var rad = radius(ps, W, Hh) * (sx || 1);
    cx.globalAlpha = a;
    if (!CLEAR.test(ps.backgroundColor)) {
      plate(cx, x, y, W, Hh, rad);
      cx.fillStyle = ps.backgroundColor;
      cx.fill();
    }
    if (bw > 0 && ps.borderTopStyle !== "none" && !CLEAR.test(ps.borderTopColor)) {
      plate(cx, x + bw / 2, y + bw / 2, W - bw, Hh - bw, Math.max(0, rad - bw / 2));
      cx.strokeStyle = ps.borderTopColor;
      cx.lineWidth = bw;
      cx.stroke();
    }
  }
  // A PICTURE THE CANVAS MAY READ is drawn as it is: every picture of the app's own is (its files
  // come from where the window itself comes from), and one carried in the page (`data:`, `blob:`).
  // One that would close the canvas to the recorder (a picture from another place; any picture
  // file when the page is opened from disk, as a test opens it) gets a quiet plate in its place.
  function readable(img) {
    if (!img.complete || !img.naturalWidth) return false;
    try {
      var u = new URL(img.currentSrc || img.src, location.href);
      if (u.protocol === "data:" || u.protocol === "blob:") return true;
      return u.protocol !== "file:" && u.protocol === location.protocol && u.host === location.host;
    } catch (e) {
      return false;
    }
  }
  var RING = /^(rgba?\([^)]+\)) 0px 0px 0px (\d+(?:\.\d+)?)px inset$/;
  var SHADOW = /^(rgba?\([^)]+\)) (-?[\d.]+)px (-?[\d.]+)px ([\d.]+)px (-?[\d.]+)px/;
  var SIDES = ["Top", "Right", "Bottom", "Left"];
  function paintDom(cx, e, alpha, R) {
    if (NOT[e.tagName] || NEVER[e.id] || e.classList.contains("rv-skip")) return;
    var s = getComputedStyle(e);
    if (s.display === "none") return;
    var op = parseFloat(s.opacity);
    var a = alpha * (isNaN(op) ? 1 : op);
    if (a < 0.012) return;
    var seen = s.visibility === "visible";
    var r = e.getBoundingClientRect();
    var W = R.W;
    var H = R.H;
    // a whole piece that is off the window (a scrolled-away message, a panel pushed aside)
    if (r.width > 0 && r.height > 0 && (r.bottom <= 0 || r.top >= H || r.right <= 0 || r.left >= W)) return;
    if (e.namespaceURI === NS) {
      if (seen && e.tagName.toLowerCase() === "svg") paintSvg(cx, e, a);
      return;
    }
    R.nodes++;
    var tag = e.tagName;
    if (tag === "CANVAS") {
      if (seen && r.width > 0 && e.width > 0) {
        cx.globalAlpha = a;
        try {
          cx.drawImage(e, r.left, r.top, r.width, r.height);
        } catch (err) {}
      }
      return;
    }
    if (e.id === "home-bg") {
      cx.globalAlpha = a;
      cx.drawImage(P.ground(W, H), 0, 0, W, H);
      return;
    }
    var rad = radius(s, r.width, r.height);
    if (seen && r.width > 0 && r.height > 0) {
      var bg = s.backgroundColor;
      cx.globalAlpha = a;
      if (!CLEAR.test(bg)) {
        // a plate's own shadow: its first one, as the page gives it (a pulled-in shadow is drawn softer)
        var sh = r.width > 30 ? SHADOW.exec(s.boxShadow || "") : null;
        if (sh && parseFloat(sh[4]) > 0 && !/inset/.test(s.boxShadow.split(/,(?![^(]*\))/)[0])) {
          var k = R.cv.width / R.W;
          var pull = parseFloat(sh[5]) < 0 ? 0.55 : 1;
          cx.shadowColor = sh[1];
          cx.shadowOffsetX = parseFloat(sh[2]) * k;
          cx.shadowOffsetY = parseFloat(sh[3]) * k * pull;
          cx.shadowBlur = parseFloat(sh[4]) * k * pull;
        }
        plate(cx, r.left, r.top, r.width, r.height, rad);
        cx.fillStyle = bg;
        cx.fill();
        cx.shadowBlur = 0;
        cx.shadowOffsetX = 0;
        cx.shadowOffsetY = 0;
        cx.shadowColor = "transparent";
      }
      if (tag === "IMG") {
        if (readable(e)) {
          cx.save();
          plate(cx, r.left, r.top, r.width, r.height, rad);
          cx.clip();
          try {
            cx.drawImage(e, r.left, r.top, r.width, r.height);
          } catch (err) {}
          cx.restore();
        } else {
          plate(cx, r.left, r.top, r.width, r.height, rad);
          cx.globalAlpha = a * 0.28;
          cx.fillStyle = s.color;
          cx.fill();
          cx.globalAlpha = a;
        }
      }
      var bt = parseFloat(s.borderTopWidth) || 0;
      var same = s.borderTopWidth === s.borderRightWidth && s.borderTopWidth === s.borderBottomWidth && s.borderTopWidth === s.borderLeftWidth && s.borderTopColor === s.borderBottomColor && s.borderTopColor === s.borderLeftColor && s.borderTopColor === s.borderRightColor;
      if (same) {
        if (bt > 0 && s.borderTopStyle !== "none" && !CLEAR.test(s.borderTopColor)) {
          plate(cx, r.left + bt / 2, r.top + bt / 2, r.width - bt, r.height - bt, Math.max(0, rad - bt / 2));
          cx.strokeStyle = s.borderTopColor;
          cx.lineWidth = bt;
          cx.stroke();
        }
      } else {
        // a rule on one side: under a heading, beside a column
        for (var i = 0; i < 4; i++) {
          var sd = SIDES[i];
          var bw = parseFloat(s["border" + sd + "Width"]) || 0;
          var bc = s["border" + sd + "Color"];
          if (!(bw > 0) || s["border" + sd + "Style"] === "none" || CLEAR.test(bc)) continue;
          cx.fillStyle = bc;
          if (sd === "Top") cx.fillRect(r.left, r.top, r.width, bw);
          else if (sd === "Bottom") cx.fillRect(r.left, r.bottom - bw, r.width, bw);
          else if (sd === "Left") cx.fillRect(r.left, r.top, bw, r.height);
          else cx.fillRect(r.right - bw, r.top, bw, r.height);
        }
      }
      var ring = RING.exec(s.boxShadow || "");
      if (ring) {
        var rw = parseFloat(ring[2]);
        var inset = bt + rw / 2;
        plate(cx, r.left + inset, r.top + inset, r.width - 2 * inset, r.height - 2 * inset, Math.max(0, rad - inset));
        cx.strokeStyle = ring[1];
        cx.lineWidth = rw;
        cx.stroke();
      }
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") paintField(cx, e, s, r, a, R);
    }
    // the home screen draws its own soft plates with these pieces; there the legend's is drawn as
    // the home screen's own recorder draws it (`home-video.js`): a soft plate of the same ground
    var wasHome = R.inHome;
    if (e.id === "home") R.inHome = true;
    if (seen && e.id === "home-legend") {
      cx.globalAlpha = a * 0.9;
      cx.shadowColor = "rgba(12,19,34,0.9)";
      cx.shadowBlur = 34 * (R.cv.width / R.W);
      plate(cx, r.left, r.top, r.width, r.height, 18);
      cx.fillStyle = "#0c1322";
      cx.fill();
      cx.shadowBlur = 0;
      cx.shadowColor = "transparent";
    }
    var kinds = R.inHome ? [] : pseudoKinds(e, R.now);
    if (seen && kinds.indexOf("::before") >= 0) paintPseudo(cx, e, s, r, a, "::before");
    var clip = (s.overflowX !== "visible" || s.overflowY !== "visible") && r.width > 0 && r.height > 0 && e !== document.body;
    if (clip) {
      cx.save();
      plate(cx, r.left, r.top, r.width, r.height, rad);
      cx.clip();
    }
    if (tag !== "TEXTAREA" && tag !== "SELECT") {
      for (var c = e.firstChild; c; c = c.nextSibling) {
        if (c.nodeType === 3) {
          if (seen) paintText(cx, c, s, a, R);
        } else if (c.nodeType === 1) paintDom(cx, c, a, R);
      }
    }
    if (clip) cx.restore();
    if (seen && kinds.indexOf("::after") >= 0) paintPseudo(cx, e, s, r, a, "::after");
    R.inHome = wasHome;
  }
  function paintFrame(R) {
    var cx = R.cx;
    var k = R.cv.width / R.W;
    cx.setTransform(1, 0, 0, 1, 0, 0);
    cx.globalAlpha = 1;
    var ground = getComputedStyle(document.documentElement).getPropertyValue("--ground").trim() || "#0c1322";
    cx.fillStyle = ground;
    cx.fillRect(0, 0, R.cv.width, R.cv.height);
    cx.setTransform(k, 0, 0, k, 0, 0);
    R.now = performance.now();
    R.nodes = 0;
    R.inHome = false;
    R.sel = selected();
    // what stands on the page itself, lowest first (the order the window stacks it in)
    var tops = [];
    for (var c = document.body.firstElementChild, i = 0; c; c = c.nextElementSibling, i++) {
      var z = parseInt(getComputedStyle(c).zIndex, 10);
      tops.push({ e: c, z: isNaN(z) ? 0 : z, i: i });
    }
    tops.sort(function (p, q) {
      return p.z - q.z || p.i - q.i;
    });
    var bs = getComputedStyle(document.body);
    if (!CLEAR.test(bs.backgroundColor)) {
      cx.fillStyle = bs.backgroundColor;
      cx.fillRect(0, 0, R.W, R.H);
    }
    for (var j = 0; j < tops.length; j++) paintDom(cx, tops[j].e, 1, R);
    cx.globalAlpha = 1;
    if (R.pointer && ptr.inside && !ptr.own) P.paintPointer(cx, ptr.x, ptr.y);
  }

  // THE MOUSE POINTER, drawn in the file when he chose it; not while it is on the recorder's own
  // pieces, which are not in the file either.
  var ptr = { x: 0, y: 0, inside: false, own: true };
  window.addEventListener(
    "pointermove",
    function (e) {
      ptr.x = e.clientX;
      ptr.y = e.clientY;
      ptr.inside = true;
      ptr.own = !!(e.target && e.target.closest && e.target.closest(".rv-skip"));
    },
    true
  );
  document.documentElement.addEventListener("pointerleave", function () {
    ptr.inside = false;
  });

  // ---------------------------------------------------------------------------------------------
  // THE RECORDER. A frame is painted as often as painting leaves the page room to answer him: no
  // more than 30 times a second (33 ms apart), and never taking more than about half of the time
  // between frames (the gap is 2.2 times what the last frames cost to paint, 125 ms at the most:
  // 8 frames a second on the heaviest screen).
  // ---------------------------------------------------------------------------------------------
  var rec = null;
  function setTime(t) {
    if (el.pillTime.textContent !== t) {
      el.pillTime.textContent = t;
      el.makeTime.textContent = t;
    }
  }
  function recStart() {
    var W = innerWidth;
    var H = innerHeight;
    // the window as it is, sharp on a Retina screen, never wider than 1,920
    var k = Math.max(1, Math.min(window.devicePixelRatio || 1, 1920 / W));
    var cv = document.createElement("canvas");
    cv.width = P.even(W * k);
    cv.height = P.even(H * k);
    // in the page, unseen: the words on it are then smoothed as the page smooths its own
    cv.className = "rv-skip rv-canvas";
    cv.setAttribute("aria-hidden", "true");
    document.body.appendChild(cv);
    var R = (rec = { cv: cv, cx: cv.getContext("2d", { alpha: false }), W: W, H: H, chunks: [], t0: performance.now(), last: 0, gap: 33, cost: 0, raf: 0, timer: 0, frames: 0, nodes: 0, now: 0, inHome: false, sel: null, voice: !!mic.stream, tone: !!mic.stream && mic.tone, pointer: opts.pointer, mr: null, mime: P.mimeType() });
    paintFrame(R);
    try {
      var stream = cv.captureStream(30);
      var tracks = stream.getVideoTracks();
      if (mic.stream) tracks = tracks.concat(mic.stream.getAudioTracks());
      R.mr = new MediaRecorder(new MediaStream(tracks), R.mime ? { mimeType: R.mime, videoBitsPerSecond: 10000000 } : undefined);
      R.mr.ondataavailable = function (e) {
        if (e.data && e.data.size) R.chunks.push(e.data);
      };
      R.mr.start(); // one whole file at stop, never slices (`home-video.js`)
    } catch (e) {
      R.mr = null; // no recorder in this webview: the sheet says so
    }
    var frame = function (now) {
      if (rec !== R) return;
      if (now - R.last >= R.gap - 2) {
        R.last = now;
        var t = performance.now();
        paintFrame(R);
        var took = performance.now() - t;
        R.cost += (took - R.cost) * 0.2;
        R.gap = Math.max(33, Math.min(125, R.cost * 2.2));
        R.frames++;
      }
      setTime(clock(now - R.t0));
      again();
    };
    // LAST IN EVERY FRAME. The home screen's picture is drawn fresh each frame and can only be
    // copied in the frame it was drawn in, after it was drawn. Asked for from a timer, this one's
    // turn always comes after the turns already waiting, the picture's among them.
    var again = function () {
      R.timer = setTimeout(function () {
        if (rec === R) R.raf = requestAnimationFrame(frame);
      }, 0);
    };
    again();
  }
  function recStop() {
    var R = rec;
    rec = null;
    if (!R) return Promise.resolve(null);
    cancelAnimationFrame(R.raf);
    clearTimeout(R.timer);
    R.cv.remove();
    if (twin) twin.remove();
    twin = null;
    var out = { ms: performance.now() - R.t0, w: R.cv.width, h: R.cv.height, voice: R.voice, tone: R.tone, pointer: R.pointer, mime: R.mime, blob: null, frames: R.frames, cost: R.cost, nodes: R.nodes, at: new Date() };
    if (!R.mr) return Promise.resolve(out);
    return new Promise(function (done) {
      var closed = false;
      var close = function (how) {
        if (closed) return;
        closed = true;
        out.ended = how; // "stopped" is a whole file
        out.blob = new Blob(R.chunks, { type: (R.mime || "video/webm").split(";")[0] });
        done(out);
      };
      R.mr.onstop = function () {
        close("stopped");
      };
      R.mr.onerror = function () {
        close("error");
      };
      // a recorder that never says it stopped: the sheet opens with what there is
      setTimeout(function () {
        close("timed out");
      }, 5000);
      try {
        R.mr.stop();
      } catch (e) {
        close("error");
      }
    });
  }

  // ---------------------------------------------------------------------------------------------
  // THE COUNT: three, two, one, 0.8 s each. Never in the file: recording starts after it.
  // ---------------------------------------------------------------------------------------------
  var timers = [];
  function later(ms, fn) {
    var t = setTimeout(function () {
      timers.splice(timers.indexOf(t), 1);
      fn();
    }, ms);
    timers.push(t);
  }
  function clearTimers() {
    timers.forEach(clearTimeout);
    timers = [];
  }
  function count(then) {
    state = "count";
    paintState();
    el.count.classList.add("on");
    var n = 3;
    var step = function () {
      if (n === 0) {
        el.count.classList.remove("on");
        return then();
      }
      el.countN.textContent = String(n);
      el.countN.classList.remove("beat");
      void el.countN.offsetWidth;
      el.countN.classList.add("beat");
      n--;
      later(800, step);
    };
    step();
  }
  function cancelCount() {
    clearTimers();
    el.count.classList.remove("on");
  }

  // ---------------------------------------------------------------------------------------------
  // RECORDING, START TO SHEET
  // ---------------------------------------------------------------------------------------------
  function begin() {
    state = "count"; // before the choices close, so the microphone is kept
    closePanel();
    // Settings closes: the video is of the app, and he opens whatever he wants to show
    if (window.RichSettings) window.RichSettings.close();
    count(function () {
      state = "rec";
      paintState();
      later(60, recStart);
    });
  }
  function finish() {
    if (state !== "rec") return;
    state = "saving"; // the file is being closed; the sheet opens when it is whole
    clearTimers();
    recStop().then(function (out) {
      paintState();
      // stopped in the moment between the count and the first frame: there is nothing to show
      if (!out) return void rest();
      openSheet(out);
    });
  }

  // ---------------------------------------------------------------------------------------------
  // THE SHEET: the video he just made, playing, and what to do with it.
  // ---------------------------------------------------------------------------------------------
  var shot = null;
  var stale = null;
  /// "RichOS-2026-11-15-at-14.32.07.mp4": when it was made, as the Mac names its own recordings.
  function fileName(out) {
    var d = out.at;
    var two = function (n) {
      return String(n).padStart(2, "0");
    };
    var ext = out.mime && out.mime.indexOf("mp4") >= 0 ? "mp4" : "webm";
    return "RichOS-" + d.getFullYear() + "-" + two(d.getMonth() + 1) + "-" + two(d.getDate()) + "-at-" + two(d.getHours()) + "." + two(d.getMinutes()) + "." + two(d.getSeconds()) + "." + ext;
  }
  function openSheet(out) {
    if (stale) URL.revokeObjectURL(stale);
    stale = null;
    shot = out;
    state = "sheet";
    mic.stop(); // the file is whole: the microphone is let go (his choice stays)
    var parts = [clock(out.ms), out.voice ? "with your voice" : "no sound", out.pointer ? "with the mouse pointer" : "no mouse pointer", fmt(out.w) + " × " + fmt(out.h)];
    if (out.blob) parts.push((out.blob.size / 1048576).toFixed(1) + " MB");
    el.sheetMeta.textContent = parts.join(" · ");
    el.sheetSaved.hidden = true;
    el.sheetSave.hidden = false;
    el.sheetSave.disabled = false;
    el.sheetDiscard.textContent = "Discard";
    el.video.style.aspectRatio = out.w + " / " + out.h;
    if (out.blob && out.blob.size) {
      el.video.src = shot.url = URL.createObjectURL(out.blob);
      // with the made-up microphone the sheet never plays sound: a check must stay silent on a Mac
      el.video.muted = !out.voice || out.tone;
      // it plays at once; where a window may not start a video with sound by itself, it stands on
      // its last second instead of its first
      var pl = el.video.play();
      if (pl && pl.catch)
        pl.catch(function () {
          if (shot === out) el.video.currentTime = Math.max(0, out.ms / 1000 - 1);
        });
      el.sheetNone.hidden = true;
      el.video.hidden = false;
    } else {
      el.video.hidden = true;
      el.sheetNone.hidden = false;
      el.sheetSave.hidden = true;
    }
    el.sheet.hidden = false;
    paintState();
    (el.sheetSave.hidden ? el.sheetAgain : el.sheetSave).focus();
  }
  function closeSheet(again) {
    if (el.sheet.hidden) return;
    el.sheet.hidden = true;
    try {
      el.video.pause();
      el.video.removeAttribute("src");
      el.video.load();
    } catch (e) {}
    // the file is let go when the next one is made, not here: the player may still be reading it
    if (shot && shot.url) stale = shot.url;
    shot = null;
    rest();
    if (again) openChoices(); // the choices again, as he left them
  }
  /// "Save video": the Mac's own save window (`home_video_save`), then where it went. His Cancel
  /// in that window changes nothing on the sheet.
  function save() {
    var out = shot;
    if (!out || !out.blob || !window.RichBridge || el.sheetSave.disabled) return;
    el.sheetSave.disabled = true;
    el.sheetSaved.hidden = true;
    var said = function (text) {
      if (shot !== out) return;
      el.sheetSaved.textContent = text;
      el.sheetSaved.hidden = false;
    };
    P.handOver(out.blob, fileName(out)).then(
      function (answer) {
        if (shot !== out) return;
        el.sheetSave.disabled = false;
        if (!answer || answer.saved !== true) return;
        said("Saved to " + answer.folder + " as " + answer.name);
        el.sheetSave.hidden = true;
        el.sheetDiscard.textContent = "Done";
        el.sheetDiscard.focus();
      },
      function (error) {
        if (shot !== out) return;
        el.sheetSave.disabled = false;
        said(typeof error === "string" && error ? error : "The video could not be saved there. Choose another folder.");
      }
    );
  }

  // ---------------------------------------------------------------------------------------------
  // BUILDING IT
  // ---------------------------------------------------------------------------------------------
  function seg(items, get, set) {
    var g = make("div", "rv-seg", "", { role: "group" });
    items.forEach(function (it) {
      var b = make("button", "", "", { type: "button" });
      b.appendChild(make("b", "", it.label));
      b.addEventListener("click", function () {
        if (state !== "choosing") return;
        set(it.key);
        g.paint();
      });
      it.b = b;
      g.appendChild(b);
    });
    g.paint = function () {
      items.forEach(function (it) {
        it.b.setAttribute("aria-pressed", String(get() === it.key));
      });
    };
    g.items = items;
    g.paint();
    return g;
  }
  function row(kind, label, group, extra) {
    var r = make("div", "rv-row");
    var l = make("div", "lab");
    l.appendChild(icon(kind));
    l.appendChild(make("span", "", label));
    if (extra) l.appendChild(extra);
    r.appendChild(l);
    group.setAttribute("aria-label", label);
    r.appendChild(group);
    return r;
  }
  /// One button, in two places: `make` in the heading of Settings, `pill` beside the settings
  /// button while it counts and records.
  function button(cls, id) {
    var b = make("button", cls + " rv-skip", "", { type: "button" });
    b.id = id;
    return b;
  }
  function build() {
    el.make = button("rv-make", "video-anywhere");
    el.make.setAttribute("aria-haspopup", "dialog");
    el.make.setAttribute("aria-expanded", "false");
    el.make.appendChild(icon("video"));
    el.make.appendChild(make("span", "rv-light", "", { "aria-hidden": "true" }));
    el.makeTime = make("span", "rv-time", "");
    el.make.appendChild(el.makeTime);
    el.makeWord = make("span", "w", "Make a video");
    el.make.appendChild(el.makeWord);
    el.make.appendChild(level());
    el.make.addEventListener("click", press);

    el.pill = button("rv-pill", "video-anywhere-stop");
    el.pill.appendChild(make("span", "rv-light", "", { "aria-hidden": "true" }));
    el.pillTime = make("span", "rv-time", "");
    el.pill.appendChild(el.pillTime);
    el.pillWord = make("span", "w", "Stop");
    el.pill.appendChild(el.pillWord);
    el.pill.appendChild(level());
    el.pill.addEventListener("click", press);

    // Escape closes the choices and the sheet (`main.js`, `data-dismiss`; the key is taken at the
    // foot of this file first, so each closes by its own way).
    el.panel = make("div", "rv-panel rv-skip", "", { id: "video-anywhere-choices", role: "dialog", "aria-label": "Make a video", "data-dismiss": "escape" });
    el.panel.hidden = true;
    el.micSeg = seg(
      [
        { key: "off", label: "Off" },
        { key: "on", label: "On" },
      ],
      function () {
        return mic.state === "on" ? "on" : "off";
      },
      function (k) {
        mic.want = k === "on";
        if (mic.want) mic.start();
        else mic.stop();
      }
    );
    el.micOn = el.micSeg.items[1].b;
    el.panel.appendChild(row("mic", "Microphone", el.micSeg, level()));
    el.micNote = make("p", "rv-mic-note", P.DENIED);
    el.micNote.hidden = true;
    el.panel.appendChild(el.micNote);
    el.panel.appendChild(
      row(
        "pointer",
        "Mouse pointer",
        seg(
          [
            { key: "shown", label: "Shown" },
            { key: "hidden", label: "Hidden" },
          ],
          function () {
            return opts.pointer ? "shown" : "hidden";
          },
          function (k) {
            opts.pointer = k === "shown";
          }
        )
      )
    );
    el.panel.appendChild(make("p", "says", "RichOS is recorded while you use it, on every screen you go to, until you press Stop."));
    el.go = make("button", "rv-main rv-go", "Start recording", { type: "button" });
    el.panel.appendChild(el.go);
    el.go.addEventListener("click", function () {
      if (state === "choosing" && mic.state !== "asking") begin();
    });

    el.count = make("div", "rv-count rv-skip", "", { role: "status" });
    el.countN = make("div", "n", "3");
    el.count.appendChild(el.countN);
    var cl = make("div", "l");
    cl.appendChild(make("span", "plain", "Recording starts"));
    var cm = make("span", "voice");
    cm.appendChild(icon("mic"));
    cm.appendChild(document.createTextNode("Your microphone is on"));
    cm.appendChild(level());
    cl.appendChild(cm);
    el.count.appendChild(cl);
    document.body.appendChild(el.count);

    // Not `aria-modal`, as on the home screen's sheet (`home-video.js`).
    el.sheet = make("div", "rv-sheet rv-skip", "", { id: "video-anywhere-sheet", role: "dialog", "aria-label": "Your video", "data-dismiss": "escape" });
    el.sheet.hidden = true;
    var card = make("div", "card");
    el.video = make("video", "", "", { controls: "", loop: "", playsinline: "" });
    card.appendChild(el.video);
    el.sheetNone = make("p", "none", "This Mac could not record the window, so there is no file to play here.");
    el.sheetNone.hidden = true;
    card.appendChild(el.sheetNone);
    var side = make("div", "side");
    side.appendChild(make("h2", "", "Your video"));
    el.sheetMeta = make("p", "meta");
    side.appendChild(el.sheetMeta);
    el.sheetSaved = make("p", "saved", "", { role: "status" });
    el.sheetSaved.hidden = true;
    side.appendChild(el.sheetSaved);
    var rw = make("div", "row");
    el.sheetSave = make("button", "rv-main", "Save video", { type: "button" });
    el.sheetAgain = make("button", "rv-plain", "Record again", { type: "button" });
    el.sheetDiscard = make("button", "rv-plain", "Discard", { type: "button" });
    rw.appendChild(el.sheetSave);
    rw.appendChild(el.sheetAgain);
    rw.appendChild(el.sheetDiscard);
    side.appendChild(rw);
    card.appendChild(side);
    el.sheet.appendChild(card);
    document.body.appendChild(el.sheet);
    el.sheetSave.addEventListener("click", save);
    el.sheetAgain.addEventListener("click", function () {
      closeSheet(true);
    });
    el.sheetDiscard.addEventListener("click", function () {
      closeSheet(false);
    });
  }
  function press() {
    if (state === "idle") openPanel();
    else if (state === "choosing") closePanel();
    else if (state === "rec") finish();
    else if (state === "count") {
      cancelCount();
      rest();
      openChoices();
    }
  }

  // ---------------------------------------------------------------------------------------------
  // WHERE IT STANDS: the heading of Settings. The panel is built again whenever something in it
  // changes (`settings-button.js`, `rebuild`), so the button is put back each time.
  // ---------------------------------------------------------------------------------------------
  function menu() {
    return wrap ? wrap.querySelector(".setmenu") : null;
  }
  function place() {
    if (!wrap) return;
    var m = menu();
    var head = m ? m.querySelector(".sm-head") : null;
    if (head && el.make.parentNode !== head) head.appendChild(el.make);
    if (el.pill.parentNode !== wrap) wrap.insertBefore(el.pill, wrap.firstChild);
    if (el.panel.parentNode !== wrap) wrap.appendChild(el.panel);
    // the choices belong to an open Settings panel
    if (state === "choosing" && !loose && (!m || m.hidden)) closePanel();
  }
  /// The choices hang 8px under their button, flush with its right end (under the settings button
  /// when they are open without Settings).
  function seat() {
    if (!el.panel || el.panel.hidden) return;
    var r = (loose ? wrap : el.make).getBoundingClientRect();
    el.panel.style.top = Math.round(r.bottom + 8) + "px";
    el.panel.style.right = Math.max(8, Math.round(innerWidth - r.right)) + "px";
  }
  function openPanel() {
    state = "choosing";
    if (mic.state === "denied") mic.state = "off";
    paintMic();
    if (mic.want && !mic.stream) mic.start(); // his choice from last time: the microphone again
    el.panel.hidden = false;
    seat();
    paintState();
    el.go.focus({ preventScroll: true });
  }
  /// From anywhere (the sheet's "Record again", the count's "Cancel"): Settings, then the choices.
  function openChoices() {
    if (homeOpen()) {
      // on the home screen the choices are the home screen's own (round 77): its button opens them
      var own = document.getElementById("home-video");
      if (own && own.getClientRects().length) {
        if (own.getAttribute("aria-expanded") !== "true") own.click();
        return;
      }
      // a home screen that has no button of its own yet (no memory of his): these choices, under
      // the settings button, so "Record again" is never a press that does nothing
      loose = true;
      return void openPanel();
    }
    if (!window.RichSettings) return;
    window.RichSettings.openMenu();
    place();
    requestAnimationFrame(function () {
      place();
      if (state === "idle" && el.make.isConnected) openPanel();
    });
  }
  function closePanel() {
    if (el.panel.hidden) return false;
    el.panel.hidden = true;
    loose = false;
    if (state === "choosing") rest();
    else paintState();
    return true;
  }
  /// Back to the button alone: the microphone is let go (his choice stays).
  function rest() {
    state = "idle";
    mic.stop();
    paintState();
  }
  function paintState() {
    var going = state === "rec";
    var counting = state === "count";
    body().classList.toggle("rv-busy", going || counting);
    body().classList.toggle("rv-rec", going);
    el.make.classList.toggle("going", going);
    el.pill.classList.toggle("going", going);
    el.make.setAttribute("aria-expanded", String(state === "choosing"));
    var word = going ? "Stop" : counting ? "Cancel" : "Make a video";
    el.makeWord.textContent = word;
    el.pillWord.textContent = going ? "Stop" : "Cancel";
    el.make.setAttribute("aria-label", going ? "Stop recording" : word);
    el.pill.setAttribute("aria-label", going ? "Stop recording" : "Cancel");
    if (!going) {
      el.makeTime.textContent = "";
      el.pillTime.textContent = "";
    }
  }

  // Escape goes one step back out of the choices, the count and the sheet. It does NOT stop a
  // recording: he is showing the app, and Escape is one of the keys he shows it with. On the
  // window, in the capture phase, so it is taken before Settings closes on the same key.
  //
  // SPACE ON THE SHEET'S PLAYER PLAYS AND PAUSES IT, as on the home screen's sheet
  // (`home-video.js`): the home screen opens Talk to Rich on Space, and this sheet stands on the
  // home screen when a recording is stopped there.
  window.addEventListener(
    "keydown",
    function (e) {
      if (!el.make) return;
      if ((e.key === " " || e.key === "Spacebar") && state === "sheet" && e.target === el.video) {
        if (e.metaKey || e.ctrlKey || e.altKey) return;
        e.preventDefault();
        e.stopImmediatePropagation();
        if (e.repeat) return;
        try {
          if (el.video.paused) {
            var pl = el.video.play();
            if (pl && pl.catch) pl.catch(function () {});
          } else el.video.pause();
        } catch (err) {}
        return;
      }
      if (e.key !== "Escape") return;
      var took = true;
      if (state === "sheet") closeSheet(false);
      else if (state === "choosing") {
        closePanel();
        if (el.make.isConnected && el.make.getClientRects().length) el.make.focus({ preventScroll: true });
      } else if (state === "count") {
        cancelCount();
        rest();
      } else took = false;
      if (took) {
        e.preventDefault();
        e.stopImmediatePropagation();
      }
    },
    true
  );
  document.addEventListener(
    "pointerdown",
    function (e) {
      if (el.panel && state === "choosing" && !el.panel.contains(e.target) && !el.make.contains(e.target)) closePanel();
    },
    true
  );
  window.addEventListener("resize", seat);

  function init() {
    wrap = document.querySelector(".settings");
    if (!wrap) return void setTimeout(init, 50);
    build();
    place();
    paintMic();
    paintState();
    new MutationObserver(place).observe(wrap, { childList: true, subtree: true, attributes: true, attributeFilter: ["hidden"] });
    // the choices stay under their button while Settings rises into place or is scrolled
    wrap.addEventListener("scroll", seat, true);
    wrap.addEventListener("animationend", seat, true);
  }
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", init);
  else init();

  return {
    state: function () {
      return { state: state, mic: mic.state, pointer: opts.pointer, frames: rec ? rec.frames : shot ? shot.frames : 0, cost: rec ? rec.cost : shot ? shot.cost : 0, nodes: rec ? rec.nodes : shot ? shot.nodes : 0, ended: shot ? shot.ended || null : null, saved: !!(el.sheetSaved && !el.sheetSaved.hidden && el.sheetSave.hidden) };
    },
  };
})();

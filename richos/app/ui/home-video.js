// MAKE A VIDEO: one button in the top right corner of the home screen that opens three choices
// (round 77, CEO 2026-11-14: "`round-77` is great. Get an engineer on Fable to build it.").
//
// The design is `richos-hq/design/mockups/rounds/round-77`. One button beside the settings button,
// labeled "Make a video", with a gold video icon; nothing on it is red until it records. Pressed,
// it opens out into a panel: the microphone off or on; whether the recording stops by itself (the
// film, "Watch it grow", played once) or when he stops it (the screen as he moves through it);
// whether the mouse pointer is in the video. Then a count of three, the recording, and a sheet with
// the video playing and "Save video", "Record again", "Discard".
//
// THE FILE IS REAL. The picture's own three canvases and whatever of the home screen is showing
// are drawn on one canvas, 30 times a second, and that canvas is recorded with the webview's own
// recorder (`MediaRecorder`), with the microphone's sound added when he chose it. The recorder's
// own button, choices, count and sheet (`hv-skip`) are never in the file. "Save video" hands the
// file to `home_video_save` (`src-tauri/src/home_video.rs`), which opens the Mac's own save window.
//
// THE MICROPHONE is asked for when he switches it on, never when recording starts, so the Mac's
// question is answered before the count. In a test run (`home_video_input` answers "tone") a quiet
// tone made in the page goes into the file only, never to the speakers, and the sheet plays muted.
//
// `home.js` builds the chrome (`build`), says when his count changes (`onCount`); `home-growth.js`
// plays the film (`grow`, `replaying`) and knows the day (`day`). Shown to a user whose picture has
// at least one memory of his own: there is nothing of his to film before that.
//
// Everything on the screen is built with `textContent`.

"use strict";

window.RichHomeVideo = (function () {
  var root = null;
  var el = {};
  var state = "idle"; // idle | choosing | count | rec | saving | sheet
  // the three choices; they stay as he left them
  var opts = { stops: "me", pointer: true };
  var NS = "http://www.w3.org/2000/svg";

  var growth = function () {
    return window.RichHomeGrowth;
  };
  var homeOpen = function () {
    return !!(window.RichHome && window.RichHome.isOpen && window.RichHome.isOpen());
  };

  function make(tag, cls, text, attrs) {
    var e = document.createElement(tag);
    if (cls) e.className = cls;
    if (text) e.textContent = text;
    if (attrs) for (var k in attrs) e.setAttribute(k, attrs[k]);
    return e;
  }
  var ICONS = {
    mic: ["M12 3.2a2.9 2.9 0 0 0-2.9 2.9v5.6a2.9 2.9 0 0 0 5.8 0V6.1A2.9 2.9 0 0 0 12 3.2z", "M5.6 11.4a6.4 6.4 0 0 0 12.8 0", "M12 17.8v3.2"],
    video: ["M6 5.5h12a3 3 0 0 1 3 3v7a3 3 0 0 1-3 3H6a3 3 0 0 1-3-3v-7a3 3 0 0 1 3-3z", "fM10.2 9v6l5-3z"],
    stops: ["M12 3.5a8.5 8.5 0 1 0 0 17a8.5 8.5 0 0 0 0-17z", "fM9.5 9.5h5v5h-5z"],
    pointer: ["M6.5 3.8l11.6 7.4-5.3 1.2-2.4 5z"],
  };
  function icon(kind, cls) {
    var s = document.createElementNS(NS, "svg");
    s.setAttribute("viewBox", "0 0 24 24");
    s.setAttribute("aria-hidden", "true");
    s.setAttribute("class", cls || "hv-ico");
    ICONS[kind].forEach(function (d) {
      var p = document.createElementNS(NS, "path");
      if (d[0] === "f") p.setAttribute("class", "f"); // a filled shape
      p.setAttribute("d", d[0] === "f" ? d.slice(1) : d);
      s.appendChild(p);
    });
    return s;
  }
  var fmt = function (n) {
    return Number(n).toLocaleString("en-US");
  };
  function clock(ms) {
    var s = Math.max(0, Math.floor(ms / 1000));
    return Math.floor(s / 60) + ":" + String(s % 60).padStart(2, "0");
  }

  // ---------------------------------------------------------------------------------------------
  // THE MICROPHONE
  // ---------------------------------------------------------------------------------------------
  // `want` is his choice, which stays as he left it; the microphone itself is held only while he is
  // choosing, counting or recording, so the Mac's microphone light is never left on behind him.
  //
  // ONE MICROPHONE FOR EACH RECORDER (`microphone`): this screen's, and the one that records the
  // whole app from the heading of Settings (round 79, `video-anywhere.js`). `paint` is called
  // whenever what the microphone is changes; `bars` answers the rows of five bars to move.
  /// "tone" in a test run, "microphone" for everybody else (`home_video.rs`). Asked once.
  var inputKind = null;
  function input() {
    if (inputKind) return inputKind;
    var bridge = window.RichBridge;
    inputKind = Promise.resolve(bridge && bridge.invoke ? bridge.invoke("home_video_input") : "microphone").then(
      function (kind) {
        return kind === "tone" ? "tone" : "microphone";
      },
      function () {
        return "microphone";
      }
    );
    return inputKind;
  }
  // THE MICROPHONE IS ASKED FOR WITHOUT ECHO CANCELING (CEO 2026-11-17: "In the very first playback
  // after a recording between second 2 and second 3 there's always like half a second of an audio
  // that cuts out. But that only happens on the first playback."). His files held that half second
  // whole, and the sheet's player decodes it whole the first time; what only the first playback
  // has is the microphone being let go as it starts. Echo canceling is what ties a microphone to
  // the Mac's speakers: with it the webview opens the microphone as a call's (the Mac's
  // voice-processing unit, which takes the sound output in with it), and letting such a microphone
  // go changes the output again while the video is already playing. A recording is not a call:
  // asked for without it, the microphone is only a microphone, and his voice is recorded as the
  // microphone hears it. Not heard on a Mac by any check (none may open a microphone or play a
  // sound); `tests/video-as-shown.js` holds both recorders to asking this way.
  var ASK = { audio: { echoCancellation: false } };
  function microphone(paint, bars) {
    var mic = { want: false, state: "off", stream: null, ac: null, an: null, buf: null, level: 0, tone: false };
    /// The made-up microphone: a 196 Hz tone that swells 1.7 times a second, into the file only.
    function tone(AC) {
      var ac = (mic.ac = new AC());
      var osc = ac.createOscillator();
      var g = ac.createGain();
      var lfo = ac.createOscillator();
      var lg = ac.createGain();
      osc.frequency.value = 196;
      g.gain.value = 0.05;
      lfo.frequency.value = 1.7;
      lg.gain.value = 0.045;
      lfo.connect(lg);
      lg.connect(g.gain);
      var dest = ac.createMediaStreamDestination();
      osc.connect(g);
      g.connect(dest);
      osc.start();
      lfo.start();
      return dest.stream;
    }
    mic.start = function () {
      if (mic.stream) return Promise.resolve(true);
      mic.state = "asking";
      paint();
      var AC = window.AudioContext || window.webkitAudioContext;
      return input()
        .then(function (kind) {
          mic.tone = kind === "tone";
          if (mic.tone) {
            if (!AC) throw new Error("no microphone");
            return tone(AC);
          }
          if (!navigator.mediaDevices || !navigator.mediaDevices.getUserMedia) throw new Error("no microphone");
          return navigator.mediaDevices.getUserMedia(ASK);
        })
        .then(
          function (stream) {
            // he switched it off again while the Mac was asking
            if (mic.state !== "asking") {
              stream.getTracks().forEach(function (t) {
                t.stop();
              });
              return false;
            }
            mic.stream = stream;
            try {
              if (AC) {
                var ac2 = mic.ac || (mic.ac = new AC());
                mic.an = ac2.createAnalyser();
                mic.an.fftSize = 512;
                mic.buf = new Uint8Array(mic.an.fftSize);
                ac2.createMediaStreamSource(stream).connect(mic.an);
              }
            } catch (e) {
              mic.an = null;
            }
            mic.state = "on";
            paint();
            return true;
          },
          function () {
            mic.want = false;
            mic.state = "denied";
            paint();
            return false;
          }
        );
    };
    mic.stop = function () {
      if (mic.stream)
        mic.stream.getTracks().forEach(function (t) {
          t.stop();
        });
      mic.stream = null;
      mic.an = null;
      if (mic.ac) {
        try {
          mic.ac.close();
        } catch (e) {}
        mic.ac = null;
      }
      mic.state = "off";
      mic.level = 0;
      paint();
    };
    var raf = 0;
    function tick() {
      raf = 0;
      if (mic.state !== "on") return;
      if (mic.an) {
        mic.an.getByteTimeDomainData(mic.buf);
        var sum = 0;
        for (var i = 0; i < mic.buf.length; i++) {
          var v = (mic.buf[i] - 128) / 128;
          sum += v * v;
        }
        var rms = Math.sqrt(sum / mic.buf.length);
        mic.level += (Math.min(1, rms * 9) - mic.level) * 0.35;
      } else mic.level = 0;
      var lit = Math.round(mic.level * 5);
      var all = bars();
      for (var k = 0; k < all.length; k++) {
        var row = all[k].children;
        for (var j = 0; j < row.length; j++) row[j].classList.toggle("on", j < Math.max(1, lit));
      }
      raf = requestAnimationFrame(tick);
    }
    /// The bars move only while there is a microphone to move them.
    mic.watch = function () {
      if (mic.state === "on" && !raf) raf = requestAnimationFrame(tick);
    };
    return mic;
  }
  var mic = microphone(
    function () {
      paintMic();
    },
    function () {
      return root ? root.querySelectorAll(".hv-level") : [];
    }
  );
  /// Five bars that move with his voice: the proof the microphone hears him.
  function level(cls) {
    var m = make("span", cls || "hv-level", "", { "aria-hidden": "true" });
    for (var i = 0; i < 5; i++) m.appendChild(make("i"));
    return m;
  }
  /// The microphone row says what the microphone is.
  function paintMic() {
    if (!root) return;
    root.classList.toggle("hv-mic-on", mic.state === "on");
    if (el.micSeg) el.micSeg.paint();
    if (el.micOn) el.micOn.querySelector("b").textContent = mic.state === "asking" ? "Asking…" : "On";
    if (el.micNote) el.micNote.hidden = mic.state !== "denied";
    mic.watch();
  }
  var DENIED = "This window was not given a microphone. The video will have no sound.";

  // ---------------------------------------------------------------------------------------------
  // THE FRAME PAINTER: the home screen, drawn on one canvas.
  // The picture is the engine's own three canvases. Everything else is read off the page as it
  // stands (where each word is, its face, its color, each plate and edge) and drawn in the same
  // place, so the file shows what the screen shows. Anything marked `hv-skip` is the recorder's
  // own control and is never in the file.
  // ---------------------------------------------------------------------------------------------
  var bgCache = null;
  function ground(W, H) {
    if (bgCache && bgCache.width === W && bgCache.height === H) return bgCache;
    var c = document.createElement("canvas");
    c.width = W;
    c.height = H;
    var x = c.getContext("2d");
    x.fillStyle = "#0c1322";
    x.fillRect(0, 0, W, H);
    // `#home-bg`, the same four lights and the vignette (`home.css`)
    var glow = function (rx, ry, cx, cy, stops) {
      x.save();
      x.translate(cx * W, cy * H);
      x.scale(rx * W, ry * H);
      var g = x.createRadialGradient(0, 0, 0, 0, 0, 1);
      stops.forEach(function (s) {
        g.addColorStop(s[0], s[1]);
      });
      x.fillStyle = g;
      x.fillRect(-60, -60, 120, 120);
      x.restore();
    };
    glow(0.3, 0.32, 0.1, 0.88, [[0, "rgba(76,96,135,0.14)"], [0.74, "rgba(76,96,135,0)"], [1, "rgba(76,96,135,0)"]]);
    glow(0.46, 0.26, 0.74, 0.04, [[0, "rgba(76,96,135,0.15)"], [0.76, "rgba(76,96,135,0)"], [1, "rgba(76,96,135,0)"]]);
    glow(0.62, 0.58, 0.54, 0.44, [[0, "rgba(20,30,52,1)"], [0.74, "rgba(20,30,52,0)"], [1, "rgba(20,30,52,0)"]]);
    glow(0.23, 0.25, 0.52, 0.57, [[0, "rgba(194,163,92,0.115)"], [0.72, "rgba(194,163,92,0)"], [1, "rgba(194,163,92,0)"]]);
    glow(0.92, 0.92, 0.5, 0.46, [[0, "rgba(0,0,0,0)"], [0.42, "rgba(0,0,0,0)"], [1, "rgba(0,0,0,0.62)"]]);
    bgCache = c;
    return c;
  }
  function radius(s, w, h) {
    var v = s.borderTopLeftRadius || "0";
    var r = v.indexOf("%") >= 0 ? (parseFloat(v) / 100) * Math.min(w, h) : parseFloat(v) || 0;
    return Math.max(0, Math.min(r, w / 2, h / 2));
  }
  function plate(cx, x, y, w, h, r) {
    cx.beginPath();
    if (cx.roundRect) cx.roundRect(x, y, w, h, r);
    else {
      cx.moveTo(x + r, y);
      cx.arcTo(x + w, y, x + w, y + h, r);
      cx.arcTo(x + w, y + h, x, y + h, r);
      cx.arcTo(x, y + h, x, y, r);
      cx.arcTo(x, y, x + w, y, r);
      cx.closePath();
    }
  }
  var CLEAR = /^rgba\(0, 0, 0, 0\)$|^transparent$/;
  var NEVER = { "home-bg": 1, "home-stage": 1, "home-grain": 1, "home-loading": 1 };
  var range = document.createRange();
  function paintSvg(cx, e, alpha) {
    var r = e.getBoundingClientRect();
    var vb = (e.getAttribute("viewBox") || "").split(/[\s,]+/).map(Number);
    if (vb.length !== 4 || !r.width) return;
    cx.save();
    cx.globalAlpha = alpha;
    cx.translate(r.left, r.top);
    cx.scale(r.width / vb[2], r.height / vb[3]);
    cx.translate(-vb[0], -vb[1]);
    var paths = e.querySelectorAll("path");
    for (var i = 0; i < paths.length; i++) {
      var ps = getComputedStyle(paths[i]);
      var p2 = new Path2D(paths[i].getAttribute("d"));
      if (ps.fill && ps.fill !== "none") {
        cx.fillStyle = ps.fill;
        cx.fill(p2);
      }
      if (ps.stroke && ps.stroke !== "none") {
        cx.strokeStyle = ps.stroke;
        cx.lineWidth = parseFloat(ps.strokeWidth) || 1;
        cx.lineCap = "round";
        cx.lineJoin = "round";
        cx.stroke(p2);
      }
    }
    cx.restore();
  }
  function paintText(cx, node, s, alpha) {
    var t = node.textContent;
    if (!/\S/.test(t)) return;
    cx.font = s.fontStyle + " " + s.fontWeight + " " + s.fontSize + " " + s.fontFamily;
    cx.fillStyle = s.color;
    cx.globalAlpha = alpha;
    var ls = parseFloat(s.letterSpacing) || 0;
    var native = "letterSpacing" in cx;
    if (native) cx.letterSpacing = ls + "px";
    var upper = s.textTransform === "uppercase";
    var re = /\S+/g;
    var m;
    while ((m = re.exec(t))) {
      range.setStart(node, m.index);
      range.setEnd(node, m.index + m[0].length);
      var rects = range.getClientRects();
      if (!rects.length) continue;
      var rc = rects[0];
      if (rc.width === 0) continue;
      var w = upper ? m[0].toUpperCase() : m[0];
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
    }
  }
  function paintDom(cx, e, alpha, top) {
    if (e.classList.contains("hv-skip") || e.tagName === "CANVAS" || NEVER[e.id]) return;
    var s = getComputedStyle(e);
    if (s.display === "none") return;
    var a = alpha * parseFloat(s.opacity);
    if (a < 0.012) return;
    var seen = s.visibility === "visible";
    var r = e.getBoundingClientRect();
    if (e.namespaceURI === NS) {
      if (seen && e.tagName.toLowerCase() === "svg") paintSvg(cx, e, a);
      return;
    }
    var rad = radius(s, r.width, r.height);
    if (seen && !top && r.width > 0 && r.height > 0) {
      var bg = s.backgroundColor;
      var bw = parseFloat(s.borderTopWidth) || 0;
      cx.globalAlpha = a;
      if (!CLEAR.test(bg)) {
        // a plate's soft edge (the app hushes the picture around its text with a shadow of the ground)
        if (s.boxShadow !== "none" && r.width > 30) {
          cx.shadowColor = "rgba(12,19,34,0.85)";
          cx.shadowBlur = 26;
        }
        plate(cx, r.left, r.top, r.width, r.height, rad);
        cx.fillStyle = bg;
        cx.fill();
        cx.shadowBlur = 0;
        cx.shadowColor = "transparent";
      }
      if (bw > 0 && !CLEAR.test(s.borderTopColor)) {
        plate(cx, r.left + bw / 2, r.top + bw / 2, r.width - bw, r.height - bw, Math.max(0, rad - bw / 2));
        cx.strokeStyle = s.borderTopColor;
        cx.lineWidth = bw;
        cx.stroke();
      }
    }
    // the legend's plate is a feathered pseudo-element; here a soft plate of the same ground
    if (seen && e.id === "home-legend") {
      cx.globalAlpha = a * 0.9;
      cx.shadowColor = "rgba(12,19,34,0.9)";
      cx.shadowBlur = 34;
      plate(cx, r.left, r.top, r.width, r.height, 18);
      cx.fillStyle = "#0c1322";
      cx.fill();
      cx.shadowBlur = 0;
      cx.shadowColor = "transparent";
    }
    var clip = !top && s.overflowX !== "visible";
    if (clip) {
      cx.save();
      plate(cx, r.left, r.top, r.width, r.height, rad);
      cx.clip();
    }
    for (var c = e.firstChild; c; c = c.nextSibling) {
      if (c.nodeType === 3) {
        if (seen) paintText(cx, c, s, a);
      } else if (c.nodeType === 1) paintDom(cx, c, a, false);
    }
    if (clip) cx.restore();
  }
  function paintFrame(R) {
    var cx = R.cx;
    var W = innerWidth;
    var H = innerHeight;
    var k = R.cv.width / R.crop.w;
    cx.setTransform(1, 0, 0, 1, 0, 0);
    cx.globalAlpha = 1;
    cx.fillStyle = "#0c1322";
    cx.fillRect(0, 0, R.cv.width, R.cv.height);
    cx.setTransform(k, 0, 0, k, -R.crop.x * k, -R.crop.y * k);
    cx.drawImage(ground(W, H), 0, 0, W, H);
    var bk = root.querySelector("#home-bokeh");
    cx.globalAlpha = parseFloat(getComputedStyle(bk).opacity) || 0;
    cx.drawImage(bk, 0, 0, W, H);
    cx.globalAlpha = 1;
    cx.drawImage(root.querySelector("#home-gl"), 0, 0, W, H);
    cx.drawImage(root.querySelector("#home-overlay"), 0, 0, W, H);
    paintDom(cx, root, 1, true);
    cx.globalAlpha = 1;
    if (R.pointer && ptr.inside && !ptr.own) paintPointer(cx, ptr.x, ptr.y);
  }

  // THE MOUSE POINTER, drawn in the file when he chose it. It is not drawn while it is on the
  // recorder's own button or choices (or on the settings button), which are not in the file
  // either, so the video never starts or ends with a pointer pressing something that is not there.
  var ptr = { x: 0, y: 0, inside: false, own: true };
  window.addEventListener(
    "pointermove",
    function (e) {
      ptr.x = e.clientX;
      ptr.y = e.clientY;
      ptr.inside = true;
      ptr.own = !!(e.target && e.target.closest && e.target.closest(".hv-skip, .settings"));
    },
    true
  );
  document.documentElement.addEventListener("pointerleave", function () {
    ptr.inside = false;
  });
  var ARROW = typeof Path2D === "function" ? new Path2D("M0 0V17.2L4.4 13.3L7.3 19.9L10 18.7L7.1 12.2H12.6Z") : null;
  function paintPointer(cx, x, y) {
    if (!ARROW) return;
    cx.save();
    cx.translate(x, y);
    cx.scale(1.2, 1.2);
    cx.shadowColor = "rgba(0,0,0,0.45)";
    cx.shadowBlur = 4;
    cx.shadowOffsetY = 1;
    cx.fillStyle = "#0b0b0c";
    cx.fill(ARROW);
    cx.shadowColor = "transparent";
    cx.lineJoin = "round";
    cx.lineWidth = 1.5;
    cx.strokeStyle = "#ffffff";
    cx.stroke(ARROW);
    cx.restore();
  }

  // ---------------------------------------------------------------------------------------------
  // THE RECORDER. A frame is painted when 31 ms or more have passed since the last one: on a
  // 60 Hz screen that is every second frame, 2 × 16.67 ms = 33.3 ms, 30 frames a second, which is
  // what the canvas is captured at.
  // ---------------------------------------------------------------------------------------------
  var rec = null;
  function mimeType() {
    var want = ["video/mp4;codecs=avc1", "video/mp4", "video/webm;codecs=vp9", "video/webm"];
    for (var i = 0; i < want.length; i++) if (window.MediaRecorder && MediaRecorder.isTypeSupported(want[i])) return want[i];
    return "";
  }
  var even = function (n) {
    return Math.max(2, Math.round(n / 2) * 2);
  };
  // THE FILE IS ASKED FOR WHILE IT RECORDS: once a second the recorder is asked for what it has
  // (`requestData`), one ask at a time, and Stop waits for an ask that is on its way, asks once
  // more, and only then stops the recorder. Both recorders use this one (`parts.tape`).
  //
  // WHY (CEO 2026-11-16, on two recordings he made in the app: "it freezes after the first few
  // seconds"). Until then the recorder was left alone until Stop, "one whole file". The Mac's own
  // webview then hands back a file as long as the recording in which the picture and the sound
  // both end early, somewhere in the first half minute, and the last frame stands to the end,
  // although every frame was painted and encoded. That happens only to a recording with sound,
  // and not in Playwright's WebKit, which is why `tests/video-whole.js` records in the Mac's own.
  // Asked once a second, the file is whole.
  //
  // WHY NOT THE RECORDER'S OWN SLICES (`start(1000)`). A Stop that lands while one of its slices
  // is on its way gets an empty last piece, and everything since the slice before is lost: most
  // recordings of four seconds came out a second short in that webview. Asked for from here, no
  // ask is on its way when the recorder is stopped.
  function tape(tracks, mime) {
    var T = { mr: null, chunks: [], asked: false, after: null, timer: 0 };
    T.mr = new MediaRecorder(new MediaStream(tracks), mime ? { mimeType: mime, videoBitsPerSecond: 10000000 } : undefined);
    T.mr.ondataavailable = function (e) {
      if (e.data && e.data.size) T.chunks.push(e.data);
      // an ask is answered even when there is nothing new to hand over (an empty piece)
      T.asked = false;
      var then = T.after;
      T.after = null;
      if (then) then();
    };
    /// Ask for what the recorder has, unless an ask is still on its way.
    T.ask = function () {
      if (T.asked || T.mr.state !== "recording") return false;
      T.asked = true;
      try {
        T.mr.requestData();
      } catch (e) {
        T.asked = false;
      }
      return T.asked;
    };
    /// Stop, and say how it ended: "stopped" is a whole file, in `T.chunks`.
    T.stop = function () {
      clearInterval(T.timer);
      return new Promise(function (done) {
        var closed = false;
        var close = function (how) {
          if (closed) return;
          closed = true;
          done(how);
        };
        /// `next` runs when the ask on its way is answered, or after two seconds without an answer.
        var answered = function (next) {
          var went = false;
          var go = function () {
            if (went) return;
            went = true;
            if (T.after === go) T.after = null;
            next();
          };
          T.after = go;
          setTimeout(go, 2000);
        };
        var end = function () {
          T.mr.onstop = function () {
            close("stopped");
          };
          T.mr.onerror = function () {
            close("error");
          };
          // a recorder that never says it stopped: the sheet opens with what there is
          setTimeout(function () {
            close("timed out");
          }, 5000);
          try {
            T.mr.stop();
          } catch (e) {
            close("error");
          }
        };
        var last = function () {
          if (T.ask()) answered(end);
          else end();
        };
        if (T.asked) answered(last);
        else last();
      });
    };
    T.mr.start();
    T.timer = setInterval(T.ask, 1000);
    return T;
  }
  function recStart() {
    var W = innerWidth;
    var H = innerHeight;
    // the window as it is, sharp on a Retina screen, never wider than 1,920
    var k = Math.max(1, Math.min(window.devicePixelRatio || 1, 1920 / W));
    var crop = { x: 0, y: 0, w: W, h: H, outW: even(W * k), outH: even(H * k) };
    var cv = document.createElement("canvas");
    cv.width = crop.outW;
    cv.height = crop.outH;
    var R = (rec = { cv: cv, cx: cv.getContext("2d", { alpha: false }), crop: crop, chunks: [], t0: performance.now(), last: 0, raf: 0, frames: 0, voice: !!mic.stream, tone: !!mic.stream && mic.tone, pointer: opts.pointer, tape: null, mime: mimeType() });
    paintFrame(R);
    try {
      var tracks = cv.captureStream(30).getVideoTracks();
      if (mic.stream) tracks = tracks.concat(mic.stream.getAudioTracks());
      R.tape = tape(tracks, R.mime);
    } catch (e) {
      R.tape = null; // no recorder in this webview: the sheet says so
    }
    var frame = function (now) {
      if (rec !== R) return;
      // he left the home screen (Talk to Rich, a company's button): the recording ends with what it has
      if (!homeOpen()) return finish();
      if (now - R.last >= 31) {
        R.last = now;
        paintFrame(R);
        R.frames++;
      }
      var t = clock(now - R.t0);
      if (el.time.textContent !== t) el.time.textContent = t;
      R.raf = requestAnimationFrame(frame);
    };
    R.raf = requestAnimationFrame(frame);
  }
  function recStop() {
    var R = rec;
    rec = null;
    if (!R) return Promise.resolve(null);
    cancelAnimationFrame(R.raf);
    var out = { ms: performance.now() - R.t0, w: R.cv.width, h: R.cv.height, voice: R.voice, tone: R.tone, pointer: R.pointer, mime: R.mime, blob: null, frames: R.frames };
    if (!R.tape) return Promise.resolve(out);
    return R.tape.stop().then(function (how) {
      out.ended = how; // "stopped" is a whole file
      out.blob = new Blob(R.tape.chunks, { type: (R.mime || "video/webm").split(";")[0] });
      return out;
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
    root.classList.add("hv-counting");
    el.count.classList.add("on");
    var n = 3;
    var step = function () {
      if (n === 0) {
        el.count.classList.remove("on");
        root.classList.remove("hv-counting");
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
    root.classList.remove("hv-counting");
  }

  // ---------------------------------------------------------------------------------------------
  // THE FILM: "Watch it grow" (`home-growth.js`), played once for the recorder.
  // ---------------------------------------------------------------------------------------------
  var filmWatch = 0;
  function playFilm(done) {
    var g = growth();
    if (g.replaying) g.grow(); // one already running is stopped first
    g.grow();
    var lastAt = 0;
    var watch = function (now) {
      if (state !== "rec" || !rec) return;
      var last = root.querySelector("#home-replay.last");
      if (last && !lastAt) lastAt = now;
      // the last frame stands for three and a half seconds, then the film is over
      if ((lastAt && now - lastAt > 3500) || (!g.replaying && now - rec.t0 > 1200)) return done();
      filmWatch = requestAnimationFrame(watch);
    };
    filmWatch = requestAnimationFrame(watch);
  }
  function endFilm() {
    cancelAnimationFrame(filmWatch);
    var g = growth();
    if (g && g.replaying) g.grow();
  }

  // ---------------------------------------------------------------------------------------------
  // RECORDING, START TO SHEET
  // ---------------------------------------------------------------------------------------------
  var take = { film: false };
  function begin() {
    var film = opts.stops === "itself";
    take = { film: film };
    state = "count"; // before the choices close, so the microphone is kept
    closePanel();
    count(function () {
      state = "rec";
      root.classList.add("hv-rec");
      root.classList.toggle("hv-rec-film", film);
      paintState();
      // a moment for the chrome that leaves the picture to be gone before the first frame is taken
      later(film ? 450 : 60, function () {
        recStart();
        if (film) playFilm(finish);
      });
    });
  }
  function finish() {
    if (state !== "rec") return;
    state = "saving"; // the file is being closed; the sheet opens when it is whole
    clearTimers();
    var was = take.film;
    recStop().then(function (out) {
      if (was) endFilm();
      root.classList.remove("hv-rec", "hv-rec-film");
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
  /// "RichOS-day-67-312-memories.mp4": the name of the file says what it is. The day is left out
  /// where the dataset cannot vouch for one (`home-growth.js`, `day`).
  function fileName(out) {
    var g = growth();
    var n = g.state().n;
    var day = g.day();
    var ext = out && out.mime && out.mime.indexOf("mp4") >= 0 ? "mp4" : "webm";
    return "RichOS-" + (day ? "day-" + day + "-" : "") + n + (n === 1 ? "-memory." : "-memories.") + ext;
  }
  var stale = null;
  function openSheet(out) {
    if (stale) URL.revokeObjectURL(stale);
    stale = null;
    shot = out;
    state = "sheet";
    mic.stop(); // the file is whole: the microphone is let go (his choice stays)
    var g = growth();
    var n = g.state().n;
    var day = g.day();
    el.sheetTitle.textContent = (day ? "Day " + fmt(day) + " · " : "") + fmt(n) + (n === 1 ? " memory" : " memories");
    var parts = [clock(out.ms), out.voice ? "with your voice" : "no sound", out.pointer ? "with the mouse pointer" : "no mouse pointer", fmt(out.w) + " × " + fmt(out.h)];
    if (out.blob) parts.push((out.blob.size / 1048576).toFixed(1) + " MB");
    el.sheetMeta.textContent = parts.join(" · ");
    el.sheetSaved.hidden = true;
    el.sheetSave.hidden = false;
    el.sheetSave.disabled = false;
    el.sheetDiscard.textContent = "Discard";
    el.video.style.aspectRatio = out.w + " / " + out.h;
    el.sheet.classList.toggle("tall", out.h > out.w);
    el.sheet.classList.toggle("square", out.h === out.w);
    var here = homeOpen();
    if (out.blob && out.blob.size) {
      el.video.src = shot.url = URL.createObjectURL(out.blob);
      // with the made-up microphone the sheet never plays sound: a check must stay silent on a Mac
      el.video.muted = !out.voice || out.tone;
      // it plays at once; where a window may not start a video with sound by itself, and where he
      // has left the home screen, it stands on its last second instead of its first, which in the
      // film is the frame worth looking at
      var held = function () {
        if (shot === out) el.video.currentTime = Math.max(0, out.ms / 1000 - 1);
      };
      if (here) {
        var pl = el.video.play();
        if (pl && pl.catch) pl.catch(held);
      } else held();
      el.sheetNone.hidden = true;
      el.video.hidden = false;
    } else {
      el.video.hidden = true;
      el.sheetNone.hidden = false;
      el.sheetSave.hidden = true;
    }
    el.sheet.hidden = false;
    root.classList.add("hv-sheet-on");
    // the keyboard stays where he went when he left this screen mid-recording
    if (here) (el.sheetSave.hidden ? el.sheetAgain : el.sheetSave).focus();
  }
  function closeSheet(again) {
    if (el.sheet.hidden) return;
    el.sheet.hidden = true;
    root.classList.remove("hv-sheet-on");
    try {
      el.video.pause();
      el.video.removeAttribute("src");
      el.video.load();
    } catch (e) {}
    // the file is let go when the next one is made, not here: the player may still be reading it
    if (shot && shot.url) stale = shot.url;
    shot = null;
    if (again) return void openPanel(); // the choices again, as he left them
    rest();
    if (homeOpen()) el.make.focus();
  }
  /// "Save video": the Mac's own save window (`home_video_save`), then where it went. His Cancel
  /// in that window changes nothing on the sheet.
  function save() {
    var out = shot;
    var bridge = window.RichBridge;
    if (!out || !out.blob || !bridge || el.sheetSave.disabled) return;
    el.sheetSave.disabled = true;
    el.sheetSaved.hidden = true;
    var said = function (text) {
      if (shot !== out) return;
      el.sheetSaved.textContent = text;
      el.sheetSaved.hidden = false;
    };
    handOver(out.blob, fileName(out)).then(
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
  /// The file and the name to suggest, handed to the Mac's own save window (`home_video_save`).
  function handOver(blob, name) {
    return blob.arrayBuffer().then(function (buffer) {
      return window.RichBridge.invoke("home_video_save", new Uint8Array(buffer), { headers: { "x-richos-name": encodeURIComponent(name) } });
    });
  }
  function buildShared() {
    el.count = make("div", "hv-count hv-skip", "", { role: "status" });
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
    root.appendChild(el.count);

    // Escape closes it (`main.js`, `data-dismiss`; on this screen the key is taken below first).
    // Not `aria-modal`: WebKit would leave the settings button, which floats over this screen,
    // out of the accessibility tree while focus is in the sheet (`tests/appearance.js`, 7c).
    el.sheet = make("div", "hv-sheet hv-skip", "", { id: "home-video-sheet", role: "dialog", "aria-label": "Your video", "data-dismiss": "escape" });
    el.sheet.hidden = true;
    var card = make("div", "card");
    el.video = make("video", "", "", { controls: "", loop: "", playsinline: "" });
    card.appendChild(el.video);
    el.sheetNone = make("p", "none", "This Mac could not record the window, so there is no file to play here.");
    el.sheetNone.hidden = true;
    card.appendChild(el.sheetNone);
    var side = make("div", "side");
    el.sheetTitle = make("h2");
    el.sheetMeta = make("p", "meta");
    side.appendChild(el.sheetTitle);
    side.appendChild(el.sheetMeta);
    el.sheetSaved = make("p", "saved", "", { role: "status" });
    el.sheetSaved.hidden = true;
    side.appendChild(el.sheetSaved);
    var row = make("div", "row");
    el.sheetSave = make("button", "hv-main", "Save video", { type: "button" });
    el.sheetAgain = make("button", "hv-plain", "Record again", { type: "button" });
    el.sheetDiscard = make("button", "hv-plain", "Discard", { type: "button" });
    row.appendChild(el.sheetSave);
    row.appendChild(el.sheetAgain);
    row.appendChild(el.sheetDiscard);
    side.appendChild(row);
    card.appendChild(side);
    el.sheet.appendChild(card);
    root.appendChild(el.sheet);
    el.sheetSave.addEventListener("click", save);
    el.sheetAgain.addEventListener("click", function () {
      closeSheet(true);
    });
    el.sheetDiscard.addEventListener("click", function () {
      closeSheet(false);
    });
  }

  // ---------------------------------------------------------------------------------------------
  // THE BUTTON AND ITS THREE CHOICES
  // ---------------------------------------------------------------------------------------------
  function seg(items, get, set) {
    var g = make("div", "hv-seg", "", { role: "group" });
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
    var r = make("div", "hv-row");
    var l = make("div", "lab");
    l.appendChild(icon(kind));
    l.appendChild(make("span", "", label));
    if (extra) l.appendChild(extra);
    r.appendChild(l);
    group.setAttribute("aria-label", label);
    r.appendChild(group);
    return r;
  }
  var SAYS = {
    itself: "The film plays once, from your first light to today and the recording stops when it ends.",
    me: "This screen is recorded while you move through it, until you press Stop.",
  };
  function buildCorner() {
    el.make = make("button", "hv-make hv-skip", "", { id: "home-video", type: "button", "aria-haspopup": "dialog", "aria-expanded": "false" });
    el.make.appendChild(icon("video"));
    el.make.appendChild(make("span", "hv-light", "", { "aria-hidden": "true" }));
    el.word = make("span", "w", "Make a video");
    el.make.appendChild(el.word);
    el.time = make("span", "hv-time", "");
    el.make.appendChild(el.time);
    el.make.appendChild(level());
    root.appendChild(el.make);
    el.make.addEventListener("click", function () {
      if (state === "idle") openPanel();
      else if (state === "choosing") closePanel();
      else if (state === "rec") finish();
      else if (state === "count") {
        cancelCount();
        openPanel();
      }
    });

    el.panel = make("div", "hv-panel hv-skip", "", { id: "home-video-choices", role: "dialog", "aria-label": "Make a video", "data-dismiss": "escape" });
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
    el.micNote = make("p", "hv-mic-note", DENIED);
    el.micNote.hidden = true;
    el.panel.appendChild(el.micNote);
    el.panel.appendChild(
      row(
        "stops",
        "Recording stops",
        seg(
          [
            { key: "itself", label: "By itself" },
            { key: "me", label: "When I stop it" },
          ],
          function () {
            return opts.stops;
          },
          function (k) {
            opts.stops = k;
            el.says.textContent = SAYS[k];
          }
        )
      )
    );
    el.says = make("p", "says", SAYS[opts.stops]);
    el.panel.appendChild(el.says);
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
    el.go = make("button", "hv-main hv-go", "Start recording", { type: "button" });
    el.panel.appendChild(el.go);
    root.appendChild(el.panel);
    el.go.addEventListener("click", function () {
      if (state === "choosing" && mic.state !== "asking") begin();
    });
  }
  function openPanel() {
    var g = growth();
    if (g && g.replaying) g.grow(); // a film already running is stopped first
    state = "choosing";
    if (mic.state === "denied") mic.state = "off";
    paintMic();
    if (mic.want && !mic.stream) mic.start(); // his choice from last time: the microphone again
    el.panel.hidden = false;
    paintState();
    el.go.focus();
  }
  function closePanel() {
    if (!el.panel || el.panel.hidden) return false;
    el.panel.hidden = true;
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
    if (!root || !el.make) return;
    root.classList.toggle("hv-busy", state === "count" || state === "rec");
    var going = state === "rec";
    el.make.classList.toggle("going", going);
    el.make.setAttribute("aria-expanded", String(state === "choosing"));
    el.word.textContent = going ? "Stop" : state === "count" ? "Cancel" : "Make a video";
    el.make.setAttribute("aria-label", going ? "Stop recording" : state === "count" ? "Cancel" : "Make a video");
    if (!going) el.time.textContent = "";
  }

  /// `home.js` (`build`): this screen's chrome, added right after the stages'.
  function build(r) {
    if (!growth()) return;
    root = r;
    buildShared();
    buildCorner();
    paintMic();
    paintState();
  }
  /// `home.js` (`onFieldCount`): the button is there once the picture has a memory of his.
  function onCount() {
    if (!root) return;
    var s = growth().state();
    root.classList.toggle("hv-on", !!s.active && s.n > 0);
  }

  /// The home screen was left (`home.js`, `hide`): the choices close and a count is called off; a
  /// recording ends with what it has (the frame loop sees it too) and its sheet waits here. The
  /// video on a sheet that is up stops playing, so one with his voice is not heard behind the
  /// screen he went to; the recording is kept, and he plays it again when he is back.
  function left() {
    if (!root) return;
    if (state === "choosing") closePanel();
    else if (state === "count") {
      cancelCount();
      rest();
    } else if (state === "rec") finish();
    else if (state === "sheet") {
      try {
        el.video.pause();
      } catch (e) {}
    }
  }

  // Escape goes one step back: out of the sheet, the choices, the count, the recording (which
  // keeps what it has). Registered before `home.js`, in the capture phase, so it is taken here first.
  //
  // SPACE ON THE SHEET'S PLAYER PLAYS AND PAUSES IT, the usual key for a video. Without this it
  // reached `home.js`, which opens Talk to Rich on Space (a player is not one of the controls it
  // leaves keys to), and the home screen went away under the video. Taken here and played or
  // paused by hand, so it does the same whether or not the webview's own controls answer Space.
  document.addEventListener(
    "keydown",
    function (e) {
      if (!root || !homeOpen()) return;
      if ((e.key === " " || e.key === "Spacebar") && state === "sheet" && e.target === el.video) {
        if (e.metaKey || e.ctrlKey || e.altKey) return;
        e.preventDefault();
        e.stopPropagation();
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
        el.make.focus();
      } else if (state === "count") {
        cancelCount();
        rest();
      } else if (state === "rec") finish();
      else took = false;
      if (took) {
        e.preventDefault();
        e.stopPropagation();
      }
    },
    true
  );
  document.addEventListener(
    "pointerdown",
    function (e) {
      if (root && state === "choosing" && !el.panel.contains(e.target) && !el.make.contains(e.target)) closePanel();
    },
    true
  );

  return {
    build: build,
    onCount: onCount,
    left: left,
    /// What the recorder of the whole app is made of too (round 79, `video-anywhere.js`).
    parts: { make: make, icon: icon, level: level, fmt: fmt, clock: clock, microphone: microphone, radius: radius, plate: plate, ground: ground, paintPointer: paintPointer, mimeType: mimeType, tape: tape, even: even, handOver: handOver, DENIED: DENIED },
    state: function () {
      return { state: state, mic: mic.state, stops: opts.stops, pointer: opts.pointer, frames: rec ? rec.frames : shot ? shot.frames : 0, ended: shot ? shot.ended || null : null, saved: !!(el.sheetSaved && !el.sheetSaved.hidden && el.sheetSave.hidden) };
    },
  };
})();

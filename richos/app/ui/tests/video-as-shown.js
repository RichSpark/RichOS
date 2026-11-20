// A VIDEO SHOWS THE APP AS THE WINDOW SHOWS IT, IN THE MAC'S OWN WEBVIEW.
//
// CEO, 2026-11-17, on four recordings he made in the app: "In the very first playback after a
// recording between second 2 and second 3 there's always like half a second of an audio that cuts
// out. But that only happens on the first playback." And on the last of them: "1) Right at the
// beginning in the input box "Talk to Rich" seems to be overlaying some other text underneath.
// 2) When the open settings are being recorded and I start scrolling down [...] a transparent
// overlay appears with the text that's normally hidden. 3) When I'm typing in text [...] the
// second text line seems to be adding one space in front of it [...] That was not the case in
// the actual app."
//
// WHY THE OTHER SUITES DID NOT SEE THE THREE IN THE PICTURE. `video-anywhere.js` records in
// Playwright's WebKit. The app's window is the Mac's own webview, and there (measured 2026-11-17):
//   1. the color of a field's placeholder cannot be asked of the page (`getComputedStyle` with
//      `::placeholder` answers the field's own color), so the message box's placeholder, which the
//      app paints clear under its two-line hint, was drawn in full ink under that hint;
//   2. what a closed `<details>` holds still answers a place and a size on the page, so Settings'
//      closed "By month" list was drawn over what stands under it;
//   3. the first word after a line break answers two places, the first an empty one at the end of
//      the line before, and the recorder then lost that line's start.
// So this suite makes his recording in the Mac's own webview (`lib/mac-webview.swift`) and reads
// what the recorder drew, word by word and where, off the recorder's own canvas.
//
// THE SOUND. All four of his files hold their sound whole (no packet missing in the first five
// seconds of any), and the sheet's player decodes it whole the first time round. What only the
// first playback has is the microphone being let go as the player starts. The microphone used to
// be asked for with the webview's echo canceling, which is the one thing that ties a microphone
// to the Mac's speakers: while it is on, the Mac's sound output runs through the microphone's own
// unit, and is taken out of it again when the microphone is let go. A recording is not a call, so
// the microphone is now asked for without it. THIS SUITE CANNOT HEAR THAT ON A MAC (nothing here
// opens a microphone or plays a sound): it checks that both recorders ask for the microphone that
// way.
//
// IT IS NOT ON THE SCREEN, TAKES NO KEYBOARD, PLAYS NO SOUND AND OPENS NO MICROPHONE
// (`lib/mac-webview.swift`, its header).
//
// Each recording and a frame of each of the three places are kept in `ui/tests/.shots/video-as-shown/`
// (the checkout's own scratch, never committed), for a person to look at.
//
// Run: node video-as-shown.js   (or `npm test` for every suite in this directory)

"use strict";

const fs = require("fs");
const path = require("path");
const { execFileSync } = require("child_process");
const { createRun, assert, assertEqual, SEED_THEME, UI_DIR } = require("./lib/harness");
const { runPage } = require("./lib/mac-webview");

const APP = path.join(UI_DIR, "index.html");
const KEEP = path.join(__dirname, ".shots", "video-as-shown");
const NOW = new Date(2026, 10, 18, 10, 0, 0);
const WEEK = Math.round(new Date(2026, 10, 16).getTime() / 86400000);
const MONTHS = [
  { month: 202611, count: 545 },
  { month: 202610, count: 1564 },
  { month: 202609, count: 2155 },
];
const HOME = { homeTeam: [{ name: "Beth", role: "Bookkeeper" }], homeField: { objects: 312, days: 67, lanes: 3 } };
const STORE = { week: WEEK, stage: 4, look: { count: 312, at: NOW.getTime() - 3 * 3600 * 1000 } };
const LINES = ["hnahinahinah", "hnohntohinh", "hoinhaonihna two words", "honihnaohinaoh"];

/// What every page of this suite starts with: its helpers, and the app with the curtain gone.
const tools = () => {
  const T = {};
  T.post = (o) => window.webkit.messageHandlers.suite.postMessage(JSON.stringify(o));
  T.sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  T.until = async (what, fn) => {
    const from = Date.now();
    for (;;) {
      let v = null;
      try {
        v = fn();
      } catch (e) {
        v = null;
      }
      if (v) return v;
      // load-bound: a minute only ends a wait that would never end (a hang); no verdict is read off it
      if (Date.now() - from > 60000) throw new Error("waited a minute for " + what);
      await T.sleep(50);
    }
  };
  T.seen = (sel) => {
    const e = document.querySelector(sel);
    if (!e || e.hidden) return false;
    const r = e.getBoundingClientRect();
    return getComputedStyle(e).display !== "none" && r.width > 0 && r.height > 0;
  };
  T.press = async (sel) => {
    await T.until(sel + " on the screen", () => T.seen(sel));
    document.querySelector(sel).click();
  };
  T.app = async () => {
    await T.until("the app", () => typeof window.RichHome === "object" && typeof window.RichTimeline === "object" && !!window.RichVideoAnywhere && !!window.__RICHOS_MOCK__);
    if (window.RichSplash) window.RichSplash.yieldNow("acceptance-suite");
    await T.until("the opening curtain to go", () => !document.querySelector("#splash"));
  };
  T.chat = async () => {
    window.RichHome.hide("suite");
    await T.until("the chat", () => !document.body.classList.contains("home-open") && !!document.querySelector("#video-anywhere"));
  };
  return T;
};

/// His recording, made again: the empty message box, Settings scrolled to its closed list, a
/// message of several lines. What the recorder drew at each is read off its own canvas.
function recording(theme, mic) {
  const drive = async (T, withMic, lines) => {
    const errors = [];
    window.addEventListener("error", (e) => errors.push(String(e.message)));
    // every word the recorder draws on its canvas, and where (in the window's own measure)
    let drawn = null;
    const fillText = CanvasRenderingContext2D.prototype.fillText;
    CanvasRenderingContext2D.prototype.fillText = function (text, x, y) {
      if (drawn && this.canvas.classList.contains("rv-canvas")) drawn.push({ text: String(text), x: Math.round(x * 10) / 10, y: Math.round(y * 10) / 10, alpha: this.globalAlpha });
      return fillText.apply(this, arguments);
    };
    const R = window.RichVideoAnywhere;
    /// The words of three whole frames painted from now, and how far into the recording that is.
    const look = async (from) => {
      await T.sleep(400); // what was just changed has settled on the page
      const first = R.state().frames;
      await T.until("the next frame", () => R.state().frames > first);
      drawn = [];
      const mark = R.state().frames;
      await T.until("three more frames", () => R.state().frames >= mark + 3);
      const words = drawn;
      drawn = null;
      const at = (performance.now() - from) / 1000;
      await T.sleep(600); // the place stays in the file long enough to be found there
      return { at, words };
    };
    try {
      await T.app();
      await T.chat();
      const out = { errors };
      await T.press("#set-btn");
      await T.until("Settings", () => T.seen("#set-menu"));
      await T.press("#video-anywhere");
      await T.until("the choices", () => T.seen("#video-anywhere-choices"));
      if (withMic) {
        Array.from(document.querySelectorAll('.rv-seg[aria-label="Microphone"] button'))
          .find((b) => b.textContent === "On")
          .click();
        await T.until("the microphone", () => R.state().mic === "on");
      }
      await T.press("#video-anywhere-choices .rv-go");
      await T.until("the recording to start", () => R.state().state === "rec");
      await T.until("the first frames", () => R.state().frames >= 2);
      const from = performance.now();

      // 1. THE EMPTY MESSAGE BOX
      const box = document.querySelector("#input");
      out.blank = { shell: document.querySelector("#input-shell").className, value: box.value, ...(await look(from)) };

      // 2. SETTINGS, SCROLLED TO ITS CLOSED LIST
      await T.press("#set-btn");
      await T.until("Settings", () => T.seen("#set-menu"));
      const list = await T.until("By month", () => document.querySelector("#set-menu details.swearmeter-months"));
      list.scrollIntoView({ block: "center" });
      const rect = (r) => ({ left: r.left, top: r.top, right: r.right, bottom: r.bottom });
      out.closed = { open: list.open, summary: rect(list.querySelector("summary").getBoundingClientRect()), items: Array.from(list.querySelectorAll("li")).map((li) => li.textContent), ...(await look(from)) };
      // opened, the list is in the file; closed again, it is out of it
      list.open = true;
      out.opened = { open: list.open, ...(await look(from)) };
      list.open = false;
      out.closedAgain = { open: list.open, ...(await look(from)) };
      window.RichSettings.close();
      await T.until("Settings to close", () => !T.seen("#set-menu"));

      // 3. A MESSAGE OF SEVERAL LINES
      box.value = lines.join("\n");
      box.dispatchEvent(new Event("input", { bubbles: true }));
      const s = getComputedStyle(box);
      const r = box.getBoundingClientRect();
      out.typed = { left: r.left + parseFloat(s.borderLeftWidth) + parseFloat(s.paddingLeft), box: rect(r), ...(await look(from)) };

      await T.press("#video-anywhere-stop");
      await T.until("the sheet", () => R.state().state === "sheet");
      out.ended = R.state().ended;
      out.muted = document.querySelector(".rv-sheet video").muted;
      await T.press(".rv-sheet .rv-main");
      await T.until("the saved file", () => window.__RICHOS_MOCK__.homeVideoSaves().length > 0);
      T.post({ file: { name: "video.mp4", base64: window.__RICHOS_MOCK__.homeVideoSaves().pop().base64 } });
      T.post({ done: out });
    } catch (e) {
      T.post({ done: { error: String((e && e.message) || e), errors } });
    }
  };
  return [
    `(${SEED_THEME.toString()})(${JSON.stringify(theme)});`,
    // SwearMeter on, so Settings has its count and, with the months below, its closed list
    `(function () { var c = JSON.parse(window.localStorage.getItem("richos-mock-config")); c.show_swearmeter = true; window.localStorage.setItem("richos-mock-config", JSON.stringify(c)); })();`,
    `window.__RICHOS_MOCK_PRESET__ = ${JSON.stringify({ homeVideoInput: "tone", swearMonths: MONTHS })};`,
    `window.addEventListener("load", function () { (${drive.toString()})((${tools.toString()})(), ${JSON.stringify(mic)}, ${JSON.stringify(LINES)}); });`,
  ].join("\n");
}

const has = (place, word) => place.words.some((w) => w.text === word && w.alpha > 0.05);

async function asShown(theme, mic) {
  const tag = theme + (mic ? "-microphone" : "-no-sound");
  const ran = await runPage(APP, recording(theme, mic), 150);
  try {
    assertEqual(ran.said.filter((m) => m.became_active), [], "the program became the active app");
    assert(ran.code === 0 && ran.done, "the page did not finish (exit " + ran.code + "): " + JSON.stringify(ran.said.slice(-4)) + " " + ran.stderr.slice(-300));
    const d = ran.done;
    assert(!d.error, "the recording could not be made: " + d.error + " " + JSON.stringify(d.errors));
    assertEqual(d.errors, [], "the page reported errors");
    assertEqual(d.ended, "stopped", "the recorder did not close the file itself");
    const file = path.join(ran.dir, "video.mp4");
    assert(fs.existsSync(file), "no file was handed to the save window");
    const probe = JSON.parse(execFileSync("ffprobe", ["-v", "error", "-show_format", "-show_streams", "-of", "json", file], { encoding: "utf8" }));
    assertEqual(probe.streams.map((s) => s.codec_name).sort(), mic ? ["aac", "h264"] : ["h264"], "the file's streams");
    if (mic) assertEqual(d.muted, true, "the sheet's player is not muted in a test run");
    fs.mkdirSync(KEEP, { recursive: true });
    fs.copyFileSync(file, path.join(KEEP, tag + ".mp4"));
    for (const [name, place] of [["1-empty-box", d.blank], ["2-settings-list-closed", d.closed], ["3-several-lines", d.typed]]) {
      execFileSync("ffmpeg", ["-v", "error", "-y", "-ss", place.at.toFixed(2), "-i", file, "-frames:v", "1", path.join(KEEP, tag + "-" + name + ".png")]);
    }

    // each of the three is looked at, whatever the one before it showed
    const faults = [];
    const hold = (ok, fault) => {
      if (!ok) faults.push(fault);
    };

    // 1. the empty message box: its two-line hint, and nothing of the placeholder under it
    assert(/\bis-blank\b/.test(d.blank.shell) && d.blank.value === "", "the message box was not empty with its hint up: " + d.blank.shell);
    assert(has(d.blank, "Type") && has(d.blank, "Click"), "the hint over the empty message box is not in the video: " + JSON.stringify(d.blank.words.slice(0, 40)));
    const under = d.blank.words.filter((w) => w.text === "Talk" || w.text === "Rich…");
    hold(under.length === 0, 'THE PLACEHOLDER IS DRAWN UNDER THE HINT: "Talk to Rich…", which the window does not show there, at ' + JSON.stringify(under.slice(0, 2)));

    // 2. Settings' closed list: its heading, and none of what it holds until it is opened
    assertEqual([d.closed.open, d.opened.open, d.closedAgain.open], [false, true, false], "the list was not closed, opened and closed again");
    assert(d.closed.items.length === MONTHS.length, "Settings' list does not hold the months: " + JSON.stringify(d.closed.items));
    const counts = MONTHS.map((m) => m.count.toLocaleString("en-US"));
    const held = (place) => counts.filter((c) => has(place, c));
    assert(has(d.closed, "By") && has(d.closed, "month"), 'the heading "By month" is not in the video with Settings scrolled to it: ' + JSON.stringify(d.closed.summary));
    hold(held(d.closed).length === 0, "WHAT A CLOSED LIST HOLDS IS DRAWN: of the months' counts, " + JSON.stringify(held(d.closed)) + " are in the video while the list is closed");
    hold(held(d.opened).length === counts.length, "the list he opened is not in the video: of its counts, only " + JSON.stringify(held(d.opened)));
    hold(held(d.closedAgain).length === 0, "the list he closed again is still in the video: " + JSON.stringify(held(d.closedAgain)));

    // 3. several lines: every line whole, and every line starting where the first does
    const inBox = d.typed.words.filter((w) => w.y > d.typed.box.top && w.y < d.typed.box.bottom).map((w) => w.text);
    const lost = [];
    const starts = [];
    for (const line of LINES) {
      const words = line.split(" ");
      for (const word of words) if (!has(d.typed, word)) lost.push(word);
      const first = d.typed.words.filter((w) => w.text === words[0]);
      if (first.length) starts.push(Math.max(...first.map((w) => Math.abs(w.x - d.typed.left))));
    }
    hold(lost.length === 0, "A LINE IS NOT WHOLE: " + JSON.stringify(lost) + " are not in the video; drawn in the box: " + JSON.stringify(Array.from(new Set(inBox))));
    hold(starts.every((off) => off <= 0.5), "A LINE STARTS IN THE WRONG PLACE: the lines start " + starts.map((o) => o.toFixed(1)).join(", ") + " px from where the box puts them (" + d.typed.left.toFixed(1) + ")");
    assertEqual(faults, [], "THE VIDEO DOES NOT SHOW WHAT THE WINDOW SHOWS");
    return `${tag}: the hint alone in the empty box (${d.blank.at.toFixed(1)} s in); "By month" closed with none of its ${MONTHS.length} counts (${d.closed.at.toFixed(1)} s), opened with all, closed again with none; ${LINES.length} lines whole and each starting where the box puts it (${d.typed.at.toFixed(1)} s); the file is ${Number(probe.format.duration).toFixed(1)} s long`;
  } finally {
    fs.rmSync(ran.dir, { recursive: true, force: true });
  }
}

function toolWindow(files) {
  const bridge = (files) => Object.defineProperty(window, "RichBridge", {
    configurable: true,
    set(bridge) {
      const invoke = bridge.invoke.bind(bridge);
      const listen = bridge.listen.bind(bridge);
      bridge.listen = async (event, handler) => {
        if (event === "rich://cleaner-recording-focus") {
          window.testToolFocus = (focused) => handler({ payload: focused });
          return () => {};
        }
        return listen(event, handler);
      };
      bridge.invoke = async (command, args, options) => {
        if (command === "cleaner_open_window") return window.testToolFocus(true);
        if (command === (files ? "richfiles_recording_frame" : "cleaner_recording_frame")) return window.testToolFrame();
        return invoke(command, args, options);
      };
      Object.defineProperty(window, "RichBridge", { value: bridge, writable: true });
    },
  });
  const drive = async (T, files) => {
    try {
      await T.app();
      await T.chat();
      if (files) {
        let focused = true;
        Object.defineProperty(document, "hasFocus", { value: () => focused });
        window.testToolFocus = (active) => {
          focused = !active;
          window.dispatchEvent(new Event(focused ? "focus" : "blur"));
        };
      } else {
        await T.until("the Cleaner focus listener", () => window.testToolFocus);
      }
      const tile = document.createElement("canvas");
      const paint = async (color) => {
        const cx = tile.getContext("2d");
        cx.fillStyle = color;
        cx.fillRect(0, 0, tile.width, tile.height);
        const jpeg = await new Promise((resolve) => tile.toBlob(resolve, "image/jpeg"));
        return new Blob([new Float32Array([NaN, NaN]), jpeg]).arrayBuffer();
      };
      const red = await paint("#cf2020");
      const green = await paint("#20b030");
      let requests = 0;
      window.testToolFrame = () => { requests++; return red; };
      await T.press("#set-btn");
      await T.press("#video-anywhere");
      await T.press("#video-anywhere-choices .rv-go");
      const canvas = await T.until("the recording canvas", () => document.querySelector(".rv-canvas"));
      if (requests) throw new Error("requested a frame before opening the tool window");
      const center = () => canvas.getContext("2d").getImageData(canvas.width / 2, canvas.height / 2, 1, 1).data;
      const isRed = ([r, g, b]) => r > 165 && g < 75 && b < 75;
      const isGreen = ([r, g, b]) => r < 75 && g > 140 && b < 100;
      // Model a covered main window whose animation callbacks are suspended.
      const animationFrame = window.requestAnimationFrame;
      window.requestAnimationFrame = () => 0;
      if (files) window.testToolFocus(true);
      else await T.press("#nr-cleaner");
      await T.until("the foreground tool window in the recording", () => isRed(center()));
      window.testToolFrame = () => { requests++; return green; };
      await T.until("the next tool picture", () => isGreen(center()));

      let resolveFrame;
      window.testToolFrame = () => new Promise((resolve) => { requests++; resolveFrame = resolve; });
      await T.until("an in-flight window frame", () => resolveFrame);
      window.requestAnimationFrame = animationFrame;
      window.testToolFocus(false);
      resolveFrame(red);
      const sent = requests;
      await T.until("the main window after the tool lost focus", () => !isRed(center()) && !isGreen(center()));
      await T.press("#video-anywhere-stop");
      await T.until("the result", () => window.RichVideoAnywhere.state().state === "sheet");
      if (requests !== sent) throw new Error("requested the tool again after it lost focus");
      T.post({ done: { ended: window.RichVideoAnywhere.state().ended } });
    } catch (e) {
      T.post({ done: { error: String((e && e.message) || e) } });
    }
  };
  return `(${bridge.toString()})(${files}); window.addEventListener("load", () => (${drive.toString()})((${tools.toString()})(), ${files}));`;
}

/// What each recorder asks the webview for when the microphone is switched on. The microphone
/// handed back is a silent one made in the page: no real microphone is opened.
function asking() {
  const drive = async (T) => {
    const errors = [];
    window.addEventListener("error", (e) => errors.push(String(e.message)));
    const asked = [];
    let silent = null;
    const stand = function (constraints) {
      asked.push(JSON.parse(JSON.stringify(constraints || null)));
      silent = silent || new AudioContext();
      return Promise.resolve(silent.createMediaStreamDestination().stream);
    };
    MediaDevices.prototype.getUserMedia = stand;
    const on = async (seg, state) => {
      Array.from(document.querySelectorAll(seg + '[aria-label="Microphone"] button'))
        .find((b) => b.textContent === "On")
        .click();
      await T.until("the microphone", () => state().mic === "on" || state().mic === "denied");
    };
    try {
      // nothing is pressed unless the stand-in is what the page will be given: no real microphone, ever
      if (navigator.mediaDevices.getUserMedia !== stand) throw new Error("the stand-in microphone could not be put in place");
      await T.app();
      window.RichHome.startField();
      await T.until("the home screen's picture", () => window.RichHome.state.field === "live" && window.RichHomeGrowth.state().arrived === true);
      const out = { errors, supported: navigator.mediaDevices.getSupportedConstraints().echoCancellation === true };
      await T.press("#home-video");
      await T.until("the home screen's choices", () => T.seen("#home-video-choices"));
      await on(".hv-seg", () => window.RichHomeVideo.state());
      out.home = asked.splice(0);
      await T.press("#home-video");
      await T.chat();
      await T.press("#set-btn");
      await T.until("Settings", () => T.seen("#set-menu"));
      await T.press("#video-anywhere");
      await T.until("the choices", () => T.seen("#video-anywhere-choices"));
      await on(".rv-seg", () => window.RichVideoAnywhere.state());
      out.anywhere = asked.splice(0);
      await T.press("#video-anywhere");
      T.post({ done: out });
    } catch (e) {
      T.post({ done: { error: String((e && e.message) || e), errors } });
    }
  };
  return [
    `(${SEED_THEME.toString()})("dark");`,
    `window.__RICHOS_MOCK_PRESET__ = ${JSON.stringify(HOME)};`,
    `window.localStorage.setItem("richos-home-growth", ${JSON.stringify(JSON.stringify(STORE))});`,
    `window.addEventListener("load", function () { (${drive.toString()})((${tools.toString()})()); });`,
  ].join("\n");
}

async function main() {
  const run = createRun("a video shows the app as the window shows it, in the Mac's own webview: the empty message box, a closed list in Settings, a message of several lines, and a microphone that is not tied to the speakers");

  await run.check("dark, with the microphone: the empty message box has its hint and no placeholder under it, Settings' closed list is not drawn until it is opened, and every line of a message starts where the box puts it", async () => asShown("dark", true));

  await run.check("light, without the microphone: the same three", async () => asShown("light", false));

  for (const files of [false, true]) {
    const tool = files ? "RichFiles" : "RichCleaner";
    await run.check(tool + " appears while it is in front and a late frame cannot return after it loses focus", async () => {
      const ran = await runPage(APP, toolWindow(files), 95);
      try {
        assert(ran.code === 0 && ran.done, "the page did not finish: " + JSON.stringify(ran.said.slice(-4)));
        assert(!ran.done.error, ran.done.error);
        assertEqual(ran.done.ended, "stopped", "the recording did not stop cleanly");
        return "read two different tool images and the return to the main window from one recording canvas";
      } finally {
        fs.rmSync(ran.dir, { recursive: true, force: true });
      }
    });
  }

  await run.check("both recorders ask for the microphone without echo canceling, so letting it go does not touch what the Mac is playing", async () => {
    const ran = await runPage(APP, asking(), 120);
    try {
      assertEqual(ran.said.filter((m) => m.became_active), [], "the program became the active app");
      assert(ran.code === 0 && ran.done, "the page did not finish (exit " + ran.code + "): " + JSON.stringify(ran.said.slice(-4)) + " " + ran.stderr.slice(-300));
      const d = ran.done;
      assert(!d.error, "the microphone could not be switched on: " + d.error + " " + JSON.stringify(d.errors));
      assertEqual(d.errors, [], "the page reported errors");
      assert(d.supported, "this webview does not know the echo canceling constraint");
      const want = [{ audio: { echoCancellation: false } }];
      assertEqual(d.home, want, "THE HOME SCREEN'S RECORDER ASKS FOR A MICROPHONE TIED TO THE SPEAKERS");
      assertEqual(d.anywhere, want, "THE RECORDER FOR EVERY OTHER SCREEN ASKS FOR A MICROPHONE TIED TO THE SPEAKERS");
      return "each asked once, for " + JSON.stringify(want[0]) + "; the webview knows the constraint";
    } finally {
      fs.rmSync(ran.dir, { recursive: true, force: true });
    }
  });

  await run.check("every placeholder the app colors is colored through --placeholder-ink, the one place the recorder can read it from", async () => {
    const off = [];
    let rules = 0;
    for (const name of fs.readdirSync(UI_DIR).filter((f) => f.endsWith(".css"))) {
      const css = fs.readFileSync(path.join(UI_DIR, name), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
      const re = /([^{}]*::placeholder[^{}]*)\{([^{}]*)\}/g;
      let m;
      while ((m = re.exec(css))) {
        const color = /(?:^|;)\s*color\s*:\s*([^;]+)/.exec(m[2]);
        if (!color) continue;
        rules++;
        if (color[1].trim() !== "var(--placeholder-ink)") off.push(name + ": " + m[1].trim().replace(/\s+/g, " ") + " { color: " + color[1].trim() + " }");
      }
    }
    assert(rules >= 10, "only " + rules + " placeholder rules were found: the search is broken");
    assertEqual(off, [], "A PLACEHOLDER THE RECORDER CANNOT READ THE COLOR OF");
    return rules + " rules";
  });

  process.exitCode = run.report() > 0 ? 1 : 0;
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

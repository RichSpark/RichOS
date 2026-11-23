// "MAKE A VIDEO" ON THE HOME SCREEN (round 77).
//
// CEO, 2026-11-14, on `richos-hq/design/mockups/rounds/round-77`: "`round-77` is great. Get an
// engineer on Fable to build it." One button in the top right corner, beside the settings button,
// that opens three choices (the microphone, whether the recording stops by itself or when he stops
// it, whether the mouse pointer is in the video), a count of three, the recording, and a sheet with
// the video playing and "Save video", "Record again", "Discard" (`home-video.js` in the app).
//
// THE RECORDER IS CHECKED BY ITS FILE. Each recording is saved with the sheet's own button; the
// browser stand-in's `home_video_save` keeps the bytes the Mac's save window would be handed, and
// the suite writes them to a scratch file and reads it back with ffprobe and ffmpeg: its streams
// (H.264 picture, AAC sound only when the microphone was on), its length against the sheet's, and
// a frame out of it for the mouse pointer and for the recorder's own button, which must not be in it.
//
// IT PLAYS NO SOUND AND OPENS NO MICROPHONE. The "microphone" is a tone made in the page that goes
// into the file only (`preset.homeVideoInput: "tone"`, the app's `RICHOS_VIDEO_INPUT_TONE`), and the
// sheet's player is muted for it; where the page asks for a real microphone (`getUserMedia`), the
// suite answers itself, with a silent stream or a refusal.
//
// Run: node home-video.js   (or `npm test` for every suite in this directory)

"use strict";

const fs = require("fs");
const os = require("os");
const path = require("path");
const { execFileSync } = require("child_process");
const { loadPlaywright, createRun, assert, assertEqual, SEED_THEME, UI_DIR } = require("./lib/harness");
const { worstContrast } = require("./lib/worst-contrast");
const png = require("./lib/png");

const APP = "file://" + path.join(UI_DIR, "index.html");
const TEAM = [
  { name: "Beth", role: "Bookkeeper" },
  { name: "Will", role: "Marketing Director" },
];
const NOW = new Date(2026, 10, 18, 10, 0, 0);
const KEY = "richos-home-growth";
const WEEK = Math.round(new Date(2026, 10, 16).getTime() / 86400000);
const field = (objects) => ({ objects, days: 67, lanes: 3 });
/// A returning user who has seen everything up to `count` memories: no moment is owed.
const seenAt = (count, stage) => ({ week: WEEK, stage, look: { count, at: NOW.getTime() - 3 * 3600 * 1000 } });
const SCRATCH = fs.mkdtempSync(path.join(os.tmpdir(), "richos-home-video-"));

/// `getUserMedia` as the suite answers it: "silent" is a microphone that hears nothing (and counts
/// the tracks it has open, so the suite can see the microphone let go), "refused" is no microphone.
function microphone(kind) {
  window.__micOpen = 0;
  window.__micAsked = 0;
  // On the prototype: a function put on `navigator.mediaDevices` itself is gone when WebKit makes
  // that object again, and the page would then ask the real one, which never answers here.
  window.MediaDevices.prototype.getUserMedia = function () {
    window.__micAsked++;
    if (kind === "refused") return Promise.reject(new Error("NotAllowedError"));
    const AC = window.AudioContext || window.webkitAudioContext;
    const ac = new AC();
    // a microphone in a silent room: samples arrive, and every one of them is zero
    const room = ac.createOscillator();
    const hush = ac.createGain();
    hush.gain.value = 0;
    const dest = ac.createMediaStreamDestination();
    room.connect(hush);
    hush.connect(dest);
    room.start();
    const stream = dest.stream;
    window.__micOpen += stream.getAudioTracks().length;
    return Promise.resolve(stream);
  };
  // (on the prototype for the same reason: a track's own `stop` is the one the page calls)
  const stop = window.MediaStreamTrack.prototype.stop;
  window.MediaStreamTrack.prototype.stop = function () {
    if (this.kind === "audio" && this.readyState === "live") window.__micOpen--;
    return stop.call(this);
  };
}

async function openApp(browser, preset, store, mic) {
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push("console: " + m.text());
  });
  page.__errors = errors;
  await page.clock.setFixedTime(NOW);
  await page.addInitScript(SEED_THEME, "dark");
  await page.addInitScript(microphone, mic || "refused");
  await page.addInitScript(
    ([v, key, st]) => {
      window.__RICHOS_MOCK_PRESET__ = v;
      if (st) window.localStorage.setItem(key, st);
    },
    [preset, KEY, store ? JSON.stringify(store) : ""]
  );
  await page.goto(APP);
  await page.waitForFunction("typeof window.RichHome === 'object' && typeof window.RichTimeline === 'object'", null, { timeout: 60000 });
  await page.evaluate(() => window.RichSplash && window.RichSplash.yieldNow("acceptance-suite"));
  await page.evaluate(() => window.RichHome.startField());
  await page.waitForFunction("window.RichHome.state.field === 'live' && !!window.__loro", null, { timeout: 60000 });
  return page;
}
/// A user with a team and `n` memories, everything he was owed already seen, the picture arrived.
async function openAt(browser, n, more, mic) {
  const stage = n >= 2500 ? 6 : n >= 1000 ? 5 : n >= 300 ? 4 : n >= 100 ? 3 : n >= 30 ? 2 : 1;
  const page = await openApp(browser, Object.assign({ homeTeam: TEAM, homeField: field(n) }, more || {}), seenAt(n, stage), mic);
  await page.waitForFunction("window.RichHomeGrowth.state().arrived === true", null, { timeout: 60000 });
  await page.waitForFunction((want) => window.__loro && window.__loro.COUNT === want, n, { timeout: 60000 });
  return page;
}

const video = (page) => page.evaluate(() => window.RichHomeVideo.state());
const until = (page, state) => page.waitForFunction((s) => window.RichHomeVideo.state().state === s, state, { timeout: 60000 });
/// The recording has run for `seconds` by its own clock on the button ("0:03").
const recorded = (page, seconds) =>
  page.waitForFunction(
    (want) => {
      const t = document.querySelector("#home-video .hv-time").textContent.split(":");
      return t.length === 2 && Number(t[0]) * 60 + Number(t[1]) >= want;
    },
    seconds,
    { timeout: 60000 }
  );
const text = (page, sel) => page.evaluate((s) => (document.querySelector(s) ? document.querySelector(s).textContent.replace(/\s+/g, " ").trim() : null), sel);
const shown = (page, sel) =>
  page.evaluate((s) => {
    const e = document.querySelector(s);
    if (!e || e.hidden) return false;
    const cs = getComputedStyle(e);
    const r = e.getBoundingClientRect();
    return cs.display !== "none" && cs.visibility === "visible" && Number(cs.opacity) > 0.5 && r.width > 0 && r.height > 0;
  }, sel);
/// The same, given the time an opening takes (the choices open over 0.24 s, the sheet over 0.3 s).
const appears = (page, sel) =>
  page
    .waitForFunction(
      (s) => {
        const e = document.querySelector(s);
        if (!e || e.hidden) return false;
        const cs = getComputedStyle(e);
        const r = e.getBoundingClientRect();
        return cs.display !== "none" && cs.visibility === "visible" && Number(cs.opacity) > 0.99 && r.width > 0 && r.height > 0;
      },
      sel,
      { timeout: 60000 }
    )
    .then(() => true, () => false);
const rect = (page, sel) =>
  page.evaluate((s) => {
    const r = document.querySelector(s).getBoundingClientRect();
    return { left: Math.round(r.left), top: Math.round(r.top), right: Math.round(r.right), bottom: Math.round(r.bottom), width: Math.round(r.width), height: Math.round(r.height) };
  }, sel);
/// Which choice of the row named `label` is pressed.
const chosen = (page, label) => page.evaluate((l) => document.querySelector('.hv-seg[aria-label="' + l + '"] button[aria-pressed="true"]').textContent, label);
const choose = (page, label, choice) =>
  page.evaluate(
    ([l, c]) => {
      const b = Array.from(document.querySelectorAll('.hv-seg[aria-label="' + l + '"] button')).find((x) => x.textContent === c);
      b.click();
    },
    [label, choice]
  );
/// Every measured line clears its floor; returns the measurements as text for the report.
async function clears(page, selectors) {
  const rows = await worstContrast(page, selectors);
  assert(rows.length > 0, "nothing was measured for " + selectors.join(", "));
  assertEqual(rows.filter((r) => !(r.ratio >= r.needs)), [], "text below its contrast floor");
  const worst = rows.reduce((a, b) => (b.ratio - b.needs < a.ratio - a.needs ? b : a));
  return `${rows.length} line(s), the nearest to its floor "${worst.text.slice(0, 28)}" ${worst.ratio}:1 (needs ${worst.needs})`;
}
/// The contrast of two computed colors (a non-text indicator against what it stands on).
const ratio = (page, selA, propA, selB, propB) =>
  page.evaluate(
    ([sa, pa, sb, pb]) => {
      const rgb = (s, p) => {
        const c = document.createElement("canvas").getContext("2d");
        c.fillStyle = "#0c1322"; // the screen's ground, under a plate that is not quite opaque
        c.fillRect(0, 0, 1, 1);
        c.fillStyle = getComputedStyle(document.querySelector(s))[p];
        c.fillRect(0, 0, 1, 1);
        return Array.from(c.getImageData(0, 0, 1, 1).data).slice(0, 3);
      };
      const lum = (v) => {
        const f = v.map((x) => {
          x /= 255;
          return x <= 0.03928 ? x / 12.92 : Math.pow((x + 0.055) / 1.055, 2.4);
        });
        return 0.2126 * f[0] + 0.7152 * f[1] + 0.0722 * f[2];
      };
      const a = lum(rgb(sa, pa));
      const b = lum(rgb(sb, pb));
      return Math.round(((Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05)) * 100) / 100;
    },
    [selA, propA, selB, propB]
  );

/// Press "Save video" and read back the file the save window was handed.
async function saved(page, tag) {
  const before = await page.evaluate(() => window.__RICHOS_MOCK__.homeVideoSaves().length);
  await page.click(".hv-sheet .hv-main");
  await page.waitForFunction((n) => window.__RICHOS_MOCK__.homeVideoSaves().length > n, before, { timeout: 60000 });
  const file = await page.evaluate(() => window.__RICHOS_MOCK__.homeVideoSaves().pop());
  const bytes = Buffer.from(file.base64, "base64");
  assertEqual(bytes.length, file.size, "the bytes read back are not the bytes handed over");
  const at = path.join(SCRATCH, tag + "-" + file.name);
  fs.writeFileSync(at, bytes);
  const probe = JSON.parse(execFileSync("ffprobe", ["-v", "error", "-show_streams", "-show_format", "-of", "json", at], { encoding: "utf8" }));
  // the whole file decodes: every frame of it is read, and a broken one is an error here
  const decoded = execFileSync("ffmpeg", ["-v", "error", "-i", at, "-map", "0:v:0", "-f", "null", "-"], { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });
  assertEqual(decoded.trim(), "", "the saved video does not decode cleanly");
  const v = probe.streams.find((s) => s.codec_type === "video");
  const a = probe.streams.find((s) => s.codec_type === "audio");
  return { name: file.name, at, size: bytes.length, seconds: Number(probe.format.duration), video: v ? { codec: v.codec_name, width: v.width, height: v.height } : null, audio: a ? a.codec_name : null };
}
/// One frame of a saved video, `seconds` in, decoded.
function frameOf(file, seconds) {
  const out = file.at + "-" + seconds + ".png";
  execFileSync("ffmpeg", ["-v", "error", "-y", "-ss", String(seconds), "-i", file.at, "-frames:v", "1", "-pix_fmt", "rgb24", out]);
  return png.decode(fs.readFileSync(out));
}
/// Is the recorder's arrow in a decoded frame with its tip at x,y (a file as wide as the window)?
/// The arrow is near-black with a white edge 1.8px wide (`home-video.js`, `paintPointer`): its left
/// edge runs straight down from the tip, and 2.4px to the right of it, 9.6px down, is its inside.
/// A light of the picture is bright in both places, the bare ground dark in both, and neither is a
/// straight white line 13px long with black beside it.
function arrowAt(img, x, y) {
  const mean = (px, py) => {
    const i = (py * img.width + px) * img.channels;
    return (img.data[i] + img.data[i + 1] + img.data[i + 2]) / 3;
  };
  // the whole of the straight edge is white and the whole of the inside beside it is dark
  let edge = 255;
  let inside = 0;
  for (let dy = 4; dy <= 16; dy++) edge = Math.min(edge, Math.max(mean(x - 1, y + dy), mean(x, y + dy)));
  for (let dy = 7; dy <= 13; dy++) inside = Math.max(inside, mean(x + 3, y + dy));
  return { edge: Math.round(edge), inside: Math.round(inside), there: edge > 130 && inside < 80 };
}
/// How many pixels in a box are the gold of the button's icon (#c2a35c, as H.264 keeps it).
function goldIn(img, x0, y0, x1, y1) {
  let n = 0;
  for (let y = y0; y < y1; y++) {
    for (let x = x0; x < x1; x++) {
      const i = (y * img.width + x) * img.channels;
      if (Math.abs(img.data[i] - 194) < 40 && Math.abs(img.data[i + 1] - 163) < 40 && Math.abs(img.data[i + 2] - 92) < 40) n++;
    }
  }
  return n;
}

async function main() {
  const run = createRun("the home screen's Make a video: round 77, one button that opens three choices and records a real file");
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();

  await run.check("the button stands in the top right corner beside the settings button, gold and not red, once he has a memory of his own", async () => {
    let page = await openApp(browser, {}, null);
    assertEqual(await shown(page, "#home-video"), false, "Make a video is on a first-run screen, which has nothing of his to film");
    await page.close();

    page = await openAt(browser, 312);
    assert(await shown(page, "#home-video"), "Make a video is not on the screen");
    assertEqual(await text(page, "#home-video"), "Make a video", "the button's label");
    assertEqual(await page.evaluate(() => document.querySelector("#home-video").getAttribute("aria-label")), "Make a video", "the button's name");
    const b = await rect(page, "#home-video");
    const s = await rect(page, "#set-btn");
    assertEqual([b.top, b.height, s.top, s.height], [18, 40, 18, 40], "the button is not level with the settings button");
    assertEqual(s.left - b.right, 12, "the button is not 12px to the left of the settings button");
    const paint = await page.evaluate(() => {
      const m = document.querySelector("#home-video");
      return {
        icon: getComputedStyle(m.querySelector(".hv-ico")).color,
        gold: getComputedStyle(document.querySelector("#home")).getPropertyValue("--gold").trim(),
        light: getComputedStyle(m.querySelector(".hv-light")).display,
        edge: getComputedStyle(m).borderTopColor,
        panel: document.querySelector("#home-video-choices").hidden,
        mics: document.querySelectorAll("#home .hv-make").length,
      };
    });
    assertEqual(paint.icon, "rgb(194, 163, 92)", "the video icon is not gold (" + paint.gold + ")");
    assertEqual(paint.light, "none", "the red light is on the button before it records");
    assertEqual([paint.panel, paint.mics], [true, 1], "one button, its choices closed");
    const contrast = await clears(page, ["#home-video"]);
    const icon = await ratio(page, "#home-video .hv-ico", "color", "#home-video", "backgroundColor");
    const edge = await ratio(page, "#home-video", "borderTopColor", "#home-video", "backgroundColor");
    assert(icon >= 3 && edge >= 3, `the icon (${icon}) or the edge (${edge}) is under 3:1 on the plate`);
    assertEqual(page.__errors, [], "the page reported errors");
    await page.close();
    return `at ${b.left},${b.top}, ${b.width}×${b.height}, 12px from the settings button; ${contrast}; icon ${icon}:1, edge ${edge}:1`;
  });

  await run.check("pressed, it opens three choices: as first opened the microphone is off, it stops when he stops it and the pointer is shown; they close by Escape, a press outside and the button", async () => {
    const page = await openAt(browser, 312);
    await page.click("#home-video");
    assert(await appears(page, "#home-video-choices"), "the choices did not open");
    assertEqual(await page.evaluate(() => document.querySelector("#home-video").getAttribute("aria-expanded")), "true", "the button does not say its choices are open");
    assertEqual(
      await page.evaluate(() => Array.from(document.querySelectorAll("#home-video-choices .hv-row .lab span:not(.hv-level)")).map((e) => e.textContent)),
      ["Microphone", "Recording stops", "Mouse pointer"],
      "the three rows"
    );
    assertEqual([await chosen(page, "Microphone"), await chosen(page, "Recording stops"), await chosen(page, "Mouse pointer")], ["Off", "When I stop it", "Shown"], "as first opened");
    assertEqual(await text(page, "#home-video-choices .says"), "This screen is recorded while you move through it, until you press Stop.", "the sentence under Recording stops");
    const p = await rect(page, "#home-video-choices");
    const b = await rect(page, "#home-video");
    assert(p.top >= b.bottom && p.right === 1440 - 18 && p.width === 452, "the choices do not open under the button: " + JSON.stringify(p));
    assertEqual(await page.evaluate(() => document.activeElement.textContent), "Start recording", "the keyboard is not on Start recording");
    const contrast = await clears(page, ["#home-video-choices .hv-row .lab", "#home-video-choices .hv-seg button b", "#home-video-choices .says", "#home-video-choices .hv-go"]);
    const pair = await ratio(page, "#home-video-choices .hv-seg", "borderTopColor", "#home-video-choices", "backgroundColor");
    const pressed = await ratio(page, '#home-video-choices .hv-seg button[aria-pressed="true"]', "backgroundColor", "#home-video-choices", "backgroundColor");
    assert(pair >= 3 && pressed >= 3, `a pair's edge (${pair}) or the chosen one (${pressed}) is under 3:1 on the panel`);

    // Closing the choices without recording restores the last recorded settings.
    await choose(page, "Recording stops", "By itself");
    await choose(page, "Mouse pointer", "Hidden");
    assertEqual(await text(page, "#home-video-choices .says"), "The film plays once, from your first light to today and the recording stops when it ends.", "the sentence for By itself");
    await page.keyboard.press("Escape");
    assertEqual([await shown(page, "#home-video-choices"), (await video(page)).state], [false, "idle"], "Escape did not close the choices");
    assertEqual(await page.evaluate(() => window.RichHome.isOpen()), true, "Escape on the choices left the home screen");
    assertEqual(await page.evaluate(() => document.activeElement.id), "home-video", "the keyboard is not back on the button");
    await page.click("#home-video");
    assertEqual([await chosen(page, "Recording stops"), await chosen(page, "Mouse pointer")], ["When I stop it", "Shown"], "unrecorded choices became the defaults");
    await page.mouse.click(500, 500);
    assertEqual(await shown(page, "#home-video-choices"), false, "a press outside did not close the choices");
    await page.click("#home-video");
    await page.click("#home-video");
    assertEqual(await shown(page, "#home-video-choices"), false, "the button did not close its own choices");
    assertEqual(await page.evaluate(() => window.RichHome.isOpen()), true, "the home screen was left");
    assertEqual(page.__errors, [], "the page reported errors");
    await page.close();
    return `452px wide under the button; ${contrast}; a pair's edge ${pair}:1, the chosen one ${pressed}:1`;
  });

  await run.check("a recording he stops, with the microphone and the pointer: the count, the red button with the time, the sheet, and a real file with picture, sound and the pointer in it", async () => {
    const page = await openAt(browser, 312, { homeVideoInput: "tone" });
    await page.mouse.move(700, 620);
    await page.click("#home-video");
    await choose(page, "Microphone", "On");
    await page.waitForFunction(() => window.RichHomeVideo.state().mic === "on", null, { timeout: 60000 });
    assertEqual(await chosen(page, "Microphone"), "On", "the microphone is not on");
    assertEqual(await page.evaluate(() => window.__micAsked), 0, "a test run's made-up microphone asked for the real one");
    assert(await shown(page, "#home-video-choices .hv-row .hv-level"), "the bars that move with his voice are not beside Microphone");
    // THE COUNT IS TIMED IN THE PAGE, on the page's own clock, from the "3" appearing to the
    // recording starting, by a watcher that is told of each change as it is made. Until 2026-11-17
    // this suite started a stopwatch of its own only after the readings below (a contrast reading
    // among them), so a busy Mac shortened the count it measured ("the count took 1.5 s"), and it
    // read the "3" a round trip after the count began.
    await page.evaluate(() => {
      const seen = (window.__count = { steps: [], at: [], rec: 0 });
      const watcher = new MutationObserver(() => {
        const state = window.RichHomeVideo.state().state;
        const n = document.querySelector(".hv-count .n");
        const t = n ? n.textContent.trim() : "";
        if (state === "count" && t && seen.steps[seen.steps.length - 1] !== t) {
          seen.steps.push(t);
          seen.at.push(performance.now());
        }
        if (state === "rec" && seen.steps.length) {
          seen.rec = performance.now();
          watcher.disconnect();
        }
      });
      watcher.observe(document.documentElement, { subtree: true, childList: true, characterData: true, attributes: true });
    });
    await page.click("#home-video-choices .hv-go");
    await until(page, "count");
    // the count: three, two, one, 0.8 s each, with the microphone said
    assert(await shown(page, ".hv-count"), "the count is not on the screen");
    assertEqual(await text(page, ".hv-count .plain"), "Recording starts", "the count's line");
    assertEqual(await text(page, ".hv-count .voice"), "Your microphone is on", "the count does not say the microphone is on");
    assertEqual(await text(page, "#home-video .w"), "Cancel", "the button during the count");
    const countContrast = await clears(page, [".hv-count .n", ".hv-count .plain", ".hv-count .voice", "#home-video .w"]);
    await until(page, "rec");
    await page.waitForFunction(() => window.__count.rec > 0, null, { timeout: 60000 });
    const count = await page.evaluate(() => window.__count);
    assertEqual(count.steps, ["3", "2", "1"], "the count's steps, from three");
    const counted = (count.rec - count.at[0]) / 1000;
    // three steps of 0.8 s are 2.4 s; a timer is never early, and two steps would be 1.6 s
    assert(counted > 2.3, "the count took " + counted.toFixed(2) + " s from the 3 to the recording, not three steps of 0.8");
    // while it records: red edge, the beating light, the time, Stop; the pointer moves over the picture
    await page.mouse.move(700, 620);
    await recorded(page, 1);
    assertEqual(await text(page, "#home-video .w"), "Stop", "the button while recording");
    assertEqual(await page.evaluate(() => document.querySelector("#home-video").getAttribute("aria-label")), "Stop recording", "the button's name while recording");
    const red = await page.evaluate(() => {
      const m = document.querySelector("#home-video");
      return [getComputedStyle(m).borderTopColor, getComputedStyle(m.querySelector(".hv-light")).display, getComputedStyle(m.querySelector(".hv-light")).backgroundColor, getComputedStyle(m.querySelector(".hv-ico")).display];
    });
    assertEqual([red[0], red[1] !== "none", red[2], red[3]], ["rgb(255, 107, 94)", true, "rgb(255, 107, 94)", "none"], "the button is not red while it records");
    assert(await shown(page, "#home-video .hv-level"), "the bars are not on the button while the microphone records");
    const recContrast = await clears(page, ["#home-video .w", "#home-video .hv-time"]);
    const redEdge = await ratio(page, "#home-video", "borderTopColor", "#home-video", "backgroundColor");
    assert(redEdge >= 3, "the red edge is " + redEdge + ":1 on the plate");
    assert(await shown(page, "#home-signals"), "the home screen's numbers left the screen for a recording he stops himself");
    await recorded(page, 4);
    await page.click("#home-video");
    await until(page, "sheet");
    // the sheet
    assert(await appears(page, ".hv-sheet"), "the sheet did not open");
    assertEqual(await text(page, ".hv-sheet h2"), "Day 67 · 312 memories", "the sheet's heading");
    const meta = await text(page, ".hv-sheet .meta");
    assert(/^0:0\d · with your voice · with the mouse pointer · 1,440 × 900 · \d+\.\d MB$/.test(meta), "the sheet's line: " + meta);
    assertEqual(await page.evaluate(() => Array.from(document.querySelectorAll(".hv-sheet .row button")).filter((b) => !b.hidden).map((b) => b.textContent)), ["Save video", "Record again", "Discard"], "the sheet's buttons");
    const player = await page.evaluate(() => {
      const v = document.querySelector(".hv-sheet video");
      return { muted: v.muted, src: v.src.slice(0, 5), hidden: v.hidden };
    });
    assertEqual(player, { muted: true, src: "blob:", hidden: false }, "the player (muted for the made-up microphone)");
    await page.waitForFunction(() => document.querySelector(".hv-sheet video").readyState >= 2 && document.querySelector(".hv-sheet video").videoWidth === 1440, null, { timeout: 60000 });
    assertEqual((await video(page)).mic, "off", "the microphone was kept after the file was whole");
    const sheetContrast = await clears(page, [".hv-sheet h2", ".hv-sheet .meta", ".hv-sheet .row button"]);
    // the file
    const file = await saved(page, "stopped");
    assertEqual(file.name, "RichOS-day-67-312-memories.mp4", "the name of the file");
    assertEqual([file.video && file.video.codec, file.video && file.video.width, file.video && file.video.height, file.audio], ["h264", 1440, 900, "aac"], "the file's streams");
    const said = Number(meta.slice(2, 4));
    assertEqual((await video(page)).ended, "stopped", "the recorder did not close the file itself");
    assert(Math.abs(file.seconds - said) <= 1.5 && file.seconds >= 4, `the file is ${file.seconds.toFixed(1)} s long and the sheet says 0:0${said}`);
    await page.waitForFunction(() => !document.querySelector(".hv-sheet .saved").hidden, null, { timeout: 60000 });
    assertEqual(await text(page, ".hv-sheet .saved"), "Saved to Desktop as RichOS-day-67-312-memories.mp4", "where it says the file went");
    assertEqual(await page.evaluate(() => Array.from(document.querySelectorAll(".hv-sheet .row button")).filter((b) => !b.hidden).map((b) => b.textContent)), ["Record again", "Done"], "the sheet's buttons after saving");
    const savedContrast = await clears(page, [".hv-sheet .saved"]);
    // a frame out of the file: the pointer is in it (a white-edged arrow at 700,620, where the
    // picture has nothing that bright), and the recorder's own button is not (no gold icon at the
    // button's left end, where the screen has one)
    const frame = frameOf(file, Math.max(1, Math.floor(file.seconds) - 1));
    const b = await rect(page, "#home-video");
    const arrow = arrowAt(frame, 700, 620);
    assert(arrow.there, "no mouse pointer in the file at 700,620: " + JSON.stringify(arrow));
    const gold = goldIn(frame, b.left + 8, b.top + 6, b.left + 40, b.bottom - 6);
    assert(gold < 6, "the recorder's own button is in the file (" + gold + " gold pixels where its icon stands)");
    await page.click(".hv-sheet .row .hv-plain:last-child");
    assertEqual([await shown(page, ".hv-sheet"), (await video(page)).state], [false, "idle"], "Done did not close the sheet");
    assertEqual(page.__errors, [], "the page reported errors");
    await page.close();
    return `count ${counted.toFixed(1)} s; ${file.name}: ${file.video.codec} ${file.video.width}×${file.video.height} + ${file.audio}, ${file.seconds.toFixed(1)} s, ${(file.size / 1048576).toFixed(1)} MB; pointer in the frame (edge ${arrow.edge}, inside ${arrow.inside}), button not (${gold}); ${countContrast}; ${recContrast}; red edge ${redEdge}:1; ${sheetContrast}; ${savedContrast}`;
  });

  await run.check("a recording that stops by itself: the film plays once with the numbers, the team and the buttons off the screen, and the file has the film, no sound and no pointer", async () => {
    const page = await openAt(browser, 312);
    await page.mouse.move(700, 620);
    await page.click("#home-video");
    await choose(page, "Recording stops", "By itself");
    await choose(page, "Mouse pointer", "Hidden");
    // Enter on Start recording starts it, and never opens the door behind it
    await page.keyboard.press("Enter");
    await until(page, "count");
    assertEqual(await shown(page, ".hv-count .voice"), false, "the count says the microphone is on when it is off");
    await until(page, "rec");
    assertEqual(await page.evaluate(() => window.RichHome.isOpen()), true, "Enter on Start recording left the home screen");
    await page.waitForFunction(() => window.RichHomeGrowth.replaying === true, null, { timeout: 60000 });
    await page.mouse.move(700, 620);
    await page.waitForFunction(() => Number(getComputedStyle(document.querySelector("#home-signals")).opacity) < 0.01, null, { timeout: 60000 });
    const off = [];
    for (const sel of ["#home-signals", "#home-entities", "#home-live", "#home-distance", "#home-grow", "#home-finished"]) if (await shown(page, sel)) off.push(sel);
    assertEqual(off, [], "still on the screen while the film is recorded");
    assert(await shown(page, "#home-brand .mark"), "whose picture it is left the screen");
    assert(await shown(page, "#home-video"), "the Stop button left the screen");
    assertEqual(await shown(page, "#home-video .hv-level"), false, "the bars are on the button with the microphone off");
    const t0 = Date.now();
    // the film's last frame: where its line stands on the screen, and what it says
    await page.waitForFunction(() => !!document.querySelector("#home-replay.last"), null, { timeout: 60000 });
    const last = await page.evaluate(() => {
      const line = document.querySelector("#home-replay .line");
      const r = document.createRange();
      r.selectNodeContents(line);
      const b = r.getBoundingClientRect();
      return { text: line.textContent, left: Math.floor(b.left), right: Math.ceil(b.right), top: Math.floor(b.top), bottom: Math.ceil(b.bottom) };
    });
    assertEqual(last.text, "Day 67 · 312 memories · The River", "the film's last frame");
    await until(page, "sheet");
    const ran = (Date.now() - t0) / 1000;
    assertEqual(await page.evaluate(() => window.RichHomeGrowth.replaying), false, "the film is still running under the sheet");
    assert(await shown(page, "#home-signals"), "the home screen's numbers did not come back after the film");
    assertEqual(await page.evaluate(() => window.__loro.COUNT), 312, "the picture is not back at today");
    const meta = await text(page, ".hv-sheet .meta");
    assert(/^0:1\d · no sound · no mouse pointer · 1,440 × 900 · \d+\.\d MB$/.test(meta), "the sheet's line: " + meta);
    const file = await saved(page, "film");
    assertEqual([file.name, file.video && file.video.codec, file.audio], ["RichOS-day-67-312-memories.mp4", "h264", null], "the film's file");
    // twelve seconds of film after half a second on the empty picture, then the last frame for three and a half
    assert(file.seconds > 14 && file.seconds < 19, "the film's file is " + file.seconds.toFixed(1) + " s long, not about 16");
    const start = frameOf(file, 1);
    const end = frameOf(file, Math.floor(file.seconds) - 1);
    const arrows = [start, frameOf(file, 8), end].map((img) => arrowAt(img, 700, 620));
    assertEqual(arrows.filter((a) => a.there), [], "a mouse pointer is in a file made with the pointer hidden");
    // THE FILM IS IN THE FILE, FIRST TO LAST. Its last frame's line ("Day 67 · 312 memories · The
    // River") stands in the file's last second where it stood on the screen: read as type-bright
    // pixels in the line's first and last fifths, where the running counter, which is shorter and
    // centered, never reaches. Three seconds in, the same two places hold no type: the film had
    // not ended there. (How bright the picture is says nothing either way: each light flashes as
    // it lands, so an early frame can be brighter than the last.)
    const type = (img) => {
      const fifth = Math.floor((last.right - last.left) / 5);
      let n = 0;
      for (const x0 of [last.left, last.right - fifth]) {
        for (let y = last.top; y < last.bottom; y++) {
          for (let x = x0; x < x0 + fifth; x++) {
            const k = (y * img.width + x) * img.channels;
            if ((img.data[k] + img.data[k + 1] + img.data[k + 2]) / 3 > 140) n++;
          }
        }
      }
      return n;
    };
    const lit = { early: type(frameOf(file, 3)), end: type(end) };
    assert(lit.end >= 150 && lit.early * 5 < lit.end, `the film's last line is not in the file's last second alone (type-bright pixels in its first and last fifths: ${lit.early} at 3 s, ${lit.end} at the end)`);
    assertEqual(page.__errors, [], "the page reported errors");
    await page.close();
    return `the film ran ${ran.toFixed(1)} s; ${file.name}: ${file.video.codec}, no sound, ${file.seconds.toFixed(1)} s, ${(file.size / 1048576).toFixed(1)} MB; the last line's type ${lit.early} at 3 s, ${lit.end} at the end; no pointer in three frames`;
  });

  await run.check("Escape goes one step back at every step, the button calls the count off, Record again opens the choices as he left them, Discard lets the video go", async () => {
    const page = await openAt(browser, 312);
    await page.click("#home-video");
    await choose(page, "Mouse pointer", "Hidden");
    await page.click("#home-video-choices .hv-go");
    await until(page, "count");
    // the button during the count: called off, the choices again
    await page.click("#home-video");
    assertEqual([(await video(page)).state, await shown(page, ".hv-count"), await appears(page, "#home-video-choices")], ["choosing", false, true], "the button did not call the count off");
    await page.click("#home-video-choices .hv-go");
    await until(page, "count");
    await page.keyboard.press("Escape");
    assertEqual([(await video(page)).state, await shown(page, ".hv-count"), await shown(page, "#home-video-choices")], ["idle", false, false], "Escape did not call the count off");
    // load-bound: the count's timers were cleared when Escape was taken; the wait is longer than the count (2.4 s) only so a timer that leaked would have fired, and a slow host cannot make a correct build fail it
    await page.waitForTimeout(2800);
    assertEqual((await video(page)).state, "idle", "a count that was called off started a recording");
    // Escape while recording stops it and keeps what it has
    await page.click("#home-video");
    await choose(page, "Mouse pointer", "Hidden");
    await page.click("#home-video-choices .hv-go");
    await until(page, "rec");
    await recorded(page, 1);
    await page.keyboard.press("Escape");
    await until(page, "sheet");
    assertEqual(await page.evaluate(() => window.RichHome.isOpen()), true, "Escape on a recording left the home screen");
    assert(/^0:0\d · no sound · no mouse pointer/.test(await text(page, ".hv-sheet .meta")), "the sheet after Escape: " + (await text(page, ".hv-sheet .meta")));
    // Record again: the choices, as he left them
    await page.click(".hv-sheet .row .hv-plain");
    assertEqual([await shown(page, ".hv-sheet"), await appears(page, "#home-video-choices"), await chosen(page, "Mouse pointer")], [false, true, "Hidden"], "Record again did not open the choices as he left them");
    await page.click("#home-video-choices .hv-go");
    await until(page, "rec");
    await recorded(page, 1);
    await page.click("#home-video");
    await until(page, "sheet");
    // Escape on the sheet discards
    await page.keyboard.press("Escape");
    assertEqual([await shown(page, ".hv-sheet"), (await video(page)).state], [false, "idle"], "Escape did not close the sheet");
    assertEqual(await page.evaluate(() => window.__RICHOS_MOCK__.homeVideoSaves().length), 0, "a video nobody saved was handed to the save window");
    assertEqual(await page.evaluate(() => window.RichHome.isOpen()), true, "the home screen was left");
    assertEqual(page.__errors, [], "the page reported errors");
    await page.close();
    return "the count called off by the button (choices again) and by Escape; Escape stopped a recording and kept it; Record again kept Hidden; Escape discarded";
  });

  await run.check("Space on the sheet's player pauses and plays the video, and the home screen stays", async () => {
    const page = await openAt(browser, 312);
    await page.click("#home-video");
    await page.click("#home-video-choices .hv-go");
    await until(page, "rec");
    await recorded(page, 1);
    await page.click("#home-video");
    await until(page, "sheet");
    assert(await appears(page, ".hv-sheet"), "the sheet did not open");
    await page.waitForFunction(() => !document.querySelector(".hv-sheet video").paused, null, { timeout: 60000 }).catch(() => {});
    assertEqual(await page.evaluate(() => document.querySelector(".hv-sheet video").paused), false, "the video does not play on the sheet");
    await page.focus(".hv-sheet video");
    assertEqual(await page.evaluate(() => document.activeElement === document.querySelector(".hv-sheet video")), true, "the keyboard is not on the player");
    await page.keyboard.press(" ");
    const after = () => page.evaluate(() => [window.RichHome.isOpen(), window.RichHomeVideo.state().state, document.querySelector(".hv-sheet video").paused]);
    assertEqual(await after(), [true, "sheet", true], "Space on the player (home open, recorder state, video paused)");
    await page.keyboard.press(" ");
    await page.waitForFunction(() => !document.querySelector(".hv-sheet video").paused, null, { timeout: 60000 }).catch(() => {});
    assertEqual(await after(), [true, "sheet", false], "Space again on the player (home open, recorder state, video paused)");
    assertEqual(page.__errors, [], "the page reported errors");
    await page.close();
    return "Space paused the video and played it again; the home screen and the sheet stayed";
  });

  await run.check("Talk to Rich during a recording ends it with what it has, and the sheet is there when he comes back", async () => {
    const page = await openAt(browser, 312);
    await page.click("#home-video");
    await page.click("#home-video-choices .hv-go");
    await until(page, "rec");
    await recorded(page, 1);
    await page.click("#home-enter");
    await until(page, "sheet");
    assertEqual(await page.evaluate(() => window.RichHome.isOpen()), false, "Talk to Rich did not open the conversation");
    assertEqual(await page.evaluate(() => document.querySelector(".hv-sheet video").paused), true, "the video plays on a screen he has left");
    assert(await page.evaluate(() => !document.querySelector("#home").contains(document.activeElement)), "the keyboard was taken back to the screen he left");
    await page.evaluate(() => window.RichHome.show("suite"));
    assert(await appears(page, ".hv-sheet"), "the sheet is not there when he comes back");
    const file = await saved(page, "left");
    assert(file.video && file.video.codec === "h264" && file.seconds >= 0.9, "what it had is not a video: " + JSON.stringify(file));
    // THE VIDEO ON THE SHEET STOPS PLAYING WHEN HE LEAVES THE HOME SCREEN FROM IT (a desk sheet opened
    // from Settings takes the home screen away): a video with his voice would go on playing aloud
    // behind the screen he went to. The recording is kept and the sheet is there when he is back.
    // (it stood still while he was away; he presses play on the player)
    await page.evaluate(() => document.querySelector(".hv-sheet video").play());
    await page.waitForFunction(() => !document.querySelector(".hv-sheet video").paused, null, { timeout: 60000 }).catch(() => {});
    assertEqual(await page.evaluate(() => document.querySelector(".hv-sheet video").paused), false, "the video does not play on the sheet he came back to");
    await page.evaluate(() => window.RichHome.hide("suite"));
    assertEqual(await page.evaluate(() => [window.RichHome.isOpen(), document.querySelector(".hv-sheet video").paused, window.RichHomeVideo.state().state]), [false, true, "sheet"], "the video went on playing behind the screen he went to, or the recording was let go");
    await page.evaluate(() => window.RichHome.show("suite"));
    assert((await appears(page, ".hv-sheet")) && (await page.evaluate(() => document.querySelector(".hv-sheet video").src.slice(0, 5))) === "blob:", "the recording is not on the sheet when he comes back");
    // a count is called off by leaving, and never starts a recording behind his back
    await page.click(".hv-sheet .row .hv-plain:last-child");
    await page.click("#home-video");
    await page.click("#home-video-choices .hv-go");
    await until(page, "count");
    await page.click("#home-enter");
    // load-bound: leaving cleared the count's timers at once; the wait is longer than the count (2.4 s) only so a timer that leaked would have fired, and a slow host cannot make a correct build fail it
    await page.waitForTimeout(3000);
    assertEqual((await video(page)).state, "idle", "a count went on after he left the home screen");
    assertEqual(page.__errors, [], "the page reported errors");
    await page.close();
    return `ended with ${file.seconds.toFixed(1)} s; the sheet waited; its video stopped playing when he left from the sheet and was kept; a count was called off by leaving`;
  });

  await run.check("the microphone is asked for when he switches it on, said plainly when it is not given, and let go whenever he is not choosing, counting or recording", async () => {
    let page = await openAt(browser, 312, null, "refused");
    await page.click("#home-video");
    assert(await appears(page, "#home-video-choices"), "the choices did not open");
    await choose(page, "Microphone", "On");
    await page.waitForFunction(() => window.RichHomeVideo.state().mic === "denied", null, { timeout: 60000 }).catch(() => {});
    assertEqual((await video(page)).mic, "denied", "a microphone that was refused");
    assertEqual(await text(page, "#home-video-choices .hv-mic-note"), "This window was not given a microphone. The video will have no sound.", "what it says with no microphone");
    assert(await shown(page, "#home-video-choices .hv-mic-note"), "the sentence is not on the screen");
    assertEqual(await chosen(page, "Microphone"), "Off", "the microphone reads On without one");
    const contrast = await clears(page, ["#home-video-choices .hv-mic-note"]);
    await page.click("#home-video-choices .hv-go");
    await until(page, "rec");
    await recorded(page, 1);
    await page.click("#home-video");
    await until(page, "sheet");
    assert(/· no sound ·/.test(await text(page, ".hv-sheet .meta")), "a video without a microphone does not say it has no sound");
    await page.close();

    page = await openAt(browser, 312, null, "silent");
    await page.click("#home-video");
    assertEqual(await page.evaluate(() => window.__micAsked), 0, "the microphone was asked for before he switched it on");
    await choose(page, "Microphone", "On");
    await page.waitForFunction(() => window.RichHomeVideo.state().mic === "on", null, { timeout: 60000 }).catch(() => {});
    assertEqual([(await video(page)).mic].concat(await page.evaluate(() => [window.__micAsked, window.__micOpen])), ["on", 1, 1], "the microphone, switched on");
    await page.keyboard.press("Escape");
    assertEqual(await page.evaluate(() => window.__micOpen), 0, "the microphone stayed open with the choices closed");
    // Choices made without recording do not change the default.
    await page.click("#home-video");
    assertEqual([await chosen(page, "Microphone"), await page.evaluate(() => window.__micOpen)], ["Off", 0], "an abandoned microphone choice became the default");
    await choose(page, "Microphone", "On");
    await page.waitForFunction(() => window.RichHomeVideo.state().mic === "on", null, { timeout: 60000 });
    assertEqual(await page.evaluate(() => window.__micOpen), 1, "the microphone did not reopen");
    await page.click("#home-video-choices .hv-go");
    await until(page, "rec");
    assertEqual(await page.evaluate(() => window.__micOpen), 1, "the microphone was let go before the recording");
    await recorded(page, 1);
    await page.click("#home-video");
    await until(page, "sheet");
    assertEqual(await page.evaluate(() => window.__micOpen), 0, "the microphone stayed open after the recording");
    assert(/· with your voice ·/.test(await text(page, ".hv-sheet .meta")), "the sheet does not say the video has his voice");
    const file = await saved(page, "voice");
    assertEqual(file.audio, "aac", "a video made with the microphone has no sound in it");
    await page.click(".hv-sheet .row .hv-plain");
    await page.waitForFunction(() => window.RichHomeVideo.state().mic === "on");
    assertEqual([await chosen(page, "Microphone"), await page.evaluate(() => window.__micOpen)], ["On", 1], "a completed video did not restore the microphone choice");
    await page.keyboard.press("Escape");
    assertEqual(await page.evaluate(() => window.__micOpen), 0, "the restored microphone stayed open");
    assertEqual(page.__errors, [], "the page reported errors");
    await page.close();
    return `no microphone given: the sentence, Off, no sound; ${contrast}; given: let go with the choices closed and after recording, restored from the completed video, ${file.audio} in the file`;
  });

  await run.check("the save window: his Cancel changes nothing on the sheet, a folder that cannot be written is said, and saving again works", async () => {
    const page = await openAt(browser, 40, { homeVideoSave: "cancel" });
    await page.click("#home-video");
    await page.click("#home-video-choices .hv-go");
    await until(page, "rec");
    await recorded(page, 1);
    await page.click("#home-video");
    await until(page, "sheet");
    assertEqual(await text(page, ".hv-sheet h2"), "Day 67 · 40 memories", "the sheet's heading at 40");
    await page.click(".hv-sheet .hv-main");
    await page.waitForFunction(() => window.__RICHOS_MOCK__.homeVideoSaves().length === 1 && !document.querySelector(".hv-sheet .hv-main").disabled, null, { timeout: 60000 });
    assertEqual(await page.evaluate(() => window.__RICHOS_MOCK__.homeVideoSaves()[0].name), "RichOS-day-67-40-memories.mp4", "the name the save window is given");
    assertEqual([await shown(page, ".hv-sheet .saved"), await shown(page, ".hv-sheet .hv-main"), await text(page, ".hv-sheet .row .hv-plain:last-child")], [false, true, "Discard"], "his Cancel changed the sheet");
    await page.evaluate(() => (window.__RICHOS_MOCK_PRESET__.homeVideoSave = "fail"));
    await page.click(".hv-sheet .hv-main");
    await page.waitForFunction(() => !document.querySelector(".hv-sheet .saved").hidden, null, { timeout: 60000 });
    assertEqual(await text(page, ".hv-sheet .saved"), "The video could not be saved there. Choose another folder.", "what it says when the folder cannot be written");
    assert(await shown(page, ".hv-sheet .hv-main"), "Save video is gone after a save that failed");
    const contrast = await clears(page, [".hv-sheet .saved"]);
    await page.evaluate(() => (window.__RICHOS_MOCK_PRESET__.homeVideoSave = "save"));
    await page.click(".hv-sheet .hv-main");
    await page.waitForFunction(() => document.querySelector(".hv-sheet .saved").textContent.indexOf("Saved to") === 0, null, { timeout: 60000 });
    assertEqual(await text(page, ".hv-sheet .saved"), "Saved to Desktop as RichOS-day-67-40-memories.mp4", "where it says the file went");
    assertEqual(page.__errors, [], "the page reported errors");
    await page.close();
    return "Cancel: the sheet as it was; a failed save: said, Save video still there; then saved; " + contrast;
  });

  await browser.close();
  fs.rmSync(SCRATCH, { recursive: true, force: true });
  process.exitCode = run.report() > 0 ? 1 : 0;
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

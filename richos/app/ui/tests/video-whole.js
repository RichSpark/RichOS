// A VIDEO OF HALF A MINUTE IS WHOLE, IN THE MAC'S OWN WEBVIEW.
//
// CEO, 2026-11-16: "the video recorder is broken both on home screen and the rest of the pages",
// and on his two test recordings: "Basically, it freezes after the first few seconds." Each of
// his files was as long as its recording, and its picture and sound ended in the first quarter
// minute; the last frame stood to the end.
//
// WHY THE OTHER TWO SUITES DID NOT SEE IT. `home-video.js` and `video-anywhere.js` record through
// the same code in Playwright's WebKit, where a recording of 30 s is whole, and none of theirs is
// longer than a few seconds. The app's window is the Mac's own webview, and there a recorder that
// is left alone until Stop loses what it recorded after the first seconds when the recording has
// sound.
// So this suite opens the app's own page in the Mac's own webview (`lib/mac-webview.swift`) and
// makes the two recordings he made: one on the home screen, one in a chat, each 31 s long, with
// the microphone on, saved with the sheet's own button and read back with ffprobe and ffmpeg.
//
// WHOLE MEANS: the file is as long as the recording, its picture and its sound both run to within
// a second and a half of the end, no two frames of the picture are more than a second apart, and
// on the home screen, whose lights never stand still, the picture of the last second is not the
// picture of three seconds before it.
//
// IT IS NOT ON THE SCREEN, TAKES NO KEYBOARD, PLAYS NO SOUND AND OPENS NO MICROPHONE. The window
// stands outside every display and the program cannot become the active app (its header); the
// "microphone" is the tone made in the page that goes into the file only
// (`preset.homeVideoInput: "tone"`), and the sheet's player is muted for it.
//
// IT NEEDS THE MAC'S HARDWARE H.264 ENCODER, which every Mac a user runs has and a test VM guest
// has not (`scripts/testvm/home-video-walk.py`): there the webview's recorder writes no picture.
//
// Run: node video-whole.js   (or `npm test` for every suite in this directory)

"use strict";

const fs = require("fs");
const path = require("path");
const { execFileSync } = require("child_process");
const { createRun, assert, assertEqual, SEED_THEME, UI_DIR } = require("./lib/harness");
const { runPage } = require("./lib/mac-webview");
const png = require("./lib/png");

const APP = path.join(UI_DIR, "index.html");
const SECONDS = 31;
const NOW = new Date(2026, 10, 18, 10, 0, 0);
const WEEK = Math.round(new Date(2026, 10, 16).getTime() / 86400000);
const PRESET = { homeTeam: [{ name: "Beth", role: "Bookkeeper" }, { name: "Will", role: "Marketing Director" }], homeField: { objects: 312, days: 67, lanes: 3 }, homeVideoInput: "tone" };
const STORE = { week: WEEK, stage: 4, look: { count: 312, at: NOW.getTime() - 3 * 3600 * 1000 } };

/// What runs in the page before anything of its own: the user the other two suites open the app
/// as, and the recording itself, made with the page's own buttons.
function scenario(where, seconds) {
  const drive = async (place, length) => {
    const post = (o) => window.webkit.messageHandlers.suite.postMessage(JSON.stringify(o));
    const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
    const until = async (what, fn) => {
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
        await sleep(50);
      }
    };
    const seen = (sel) => {
      const e = document.querySelector(sel);
      if (!e || e.hidden) return false;
      const r = e.getBoundingClientRect();
      return getComputedStyle(e).display !== "none" && r.width > 0 && r.height > 0;
    };
    const press = async (sel) => {
      await until(sel + " on the screen", () => seen(sel));
      document.querySelector(sel).click();
    };
    const errors = [];
    window.addEventListener("error", (e) => errors.push(String(e.message)));
    try {
      await until("the app", () => typeof window.RichHome === "object" && typeof window.RichTimeline === "object" && !!window.RichVideoAnywhere && !!window.__RICHOS_MOCK__);
      if (window.RichSplash) window.RichSplash.yieldNow("acceptance-suite");
      window.RichHome.startField();
      await until("the home screen's picture", () => window.RichHome.state.field === "live" && !!window.__loro && window.RichHomeGrowth.state().arrived === true);
      await until("the opening curtain to go", () => !document.querySelector("#splash"));
      const home = place === "home";
      const R = home ? window.RichHomeVideo : window.RichVideoAnywhere;
      const seg = home ? ".hv-seg" : ".rv-seg";
      if (home) await press("#home-video");
      else {
        window.RichHome.hide("suite");
        await until("the chat", () => !document.body.classList.contains("home-open") && !!document.querySelector("#video-anywhere"));
        await press("#set-btn");
        await until("Settings", () => seen("#set-menu"));
        await press("#video-anywhere");
      }
      await until("the choices", () => seen(home ? "#home-video-choices" : "#video-anywhere-choices"));
      Array.from(document.querySelectorAll(seg + '[aria-label="Microphone"] button'))
        .find((b) => b.textContent === "On")
        .click();
      await until("the microphone", () => R.state().mic === "on");
      // THE LENGTH IS READ OFF ONE CLOCK, THE ONE THE RECORDER COUNTS ON (`performance.now()`), and
      // the wait ends when that clock says so. Until 2026-11-17 it was a `setTimeout` of 31 s
      // timed with `Date.now()`: two clocks, and in that night's build, on a loaded Mac, the
      // second said a hair under 31 s had passed when the first fired ("recorded 31.0 s" and
      // "stopped early" in one line). The count starts once the recorder has painted a frame, which
      // is after its own clock started, so its own length is at least `length` too and the sheet
      // must say so.
      const goAt = performance.now();
      await press(home ? "#home-video-choices .hv-go" : "#video-anywhere-choices .rv-go");
      await until("the recording to start", () => R.state().state === "rec" && R.state().frames > 0);
      const from = performance.now();
      while (performance.now() - from < length * 1000) await sleep(Math.min(250, Math.max(1, length * 1000 - (performance.now() - from))));
      const framesPainted = R.state().frames;
      await press(home ? "#home-video" : "#video-anywhere-stop");
      const pressed = (performance.now() - from) / 1000;
      const sinceGo = (performance.now() - goAt) / 1000;
      await until("the sheet", () => R.state().state === "sheet");
      const sheetAfter = (performance.now() - from) / 1000 - pressed;
      const meta = document.querySelector(home ? ".hv-sheet .meta" : ".rv-sheet .meta").textContent;
      await press(home ? ".hv-sheet .hv-main" : ".rv-sheet .rv-main");
      await until("the saved file", () => window.__RICHOS_MOCK__.homeVideoSaves().length > 0);
      const file = window.__RICHOS_MOCK__.homeVideoSaves().pop();
      post({ file: { name: "video.mp4", base64: file.base64 } });
      post({ done: { name: file.name, size: file.size, pressed, sinceGo, sheetAfter, meta, framesPainted, ended: R.state().ended, errors } });
    } catch (e) {
      post({ done: { error: String((e && e.message) || e), errors } });
    }
  };
  return [
    `(${SEED_THEME.toString()})("dark");`,
    `window.__RICHOS_MOCK_PRESET__ = ${JSON.stringify(PRESET)};`,
    `window.localStorage.setItem("richos-home-growth", ${JSON.stringify(JSON.stringify(STORE))});`,
    `window.addEventListener("load", function () { (${drive.toString()})(${JSON.stringify(where)}, ${seconds}); });`,
  ].join("\n");
}

const packets = (file, stream) =>
  execFileSync("ffprobe", ["-v", "error", "-select_streams", stream, "-show_entries", "packet=pts_time", "-of", "csv=p=0", file], { encoding: "utf8" })
    .split("\n")
    .map((l) => parseFloat(l))
    .filter((t) => !isNaN(t))
    .sort((a, b) => a - b);
function frameAt(file, seconds) {
  const out = file + "-" + seconds.toFixed(2) + ".png";
  execFileSync("ffmpeg", ["-v", "error", "-y", "-ss", seconds.toFixed(2), "-i", file, "-frames:v", "1", "-pix_fmt", "rgb24", out]);
  return png.decode(fs.readFileSync(out));
}
/// How many of every 1,000 pixels differ by more than a little between two frames.
function differ(a, b) {
  let n = 0;
  const px = a.width * a.height;
  for (let i = 0; i < px; i++) {
    const k = i * a.data.length / px;
    if (Math.abs(a.data[k] - b.data[k]) + Math.abs(a.data[k + 1] - b.data[k + 1]) + Math.abs(a.data[k + 2] - b.data[k + 2]) > 24) n++;
  }
  return Math.round((n / px) * 1000);
}

/// Make one recording in the Mac's own webview and read its file back.
async function record(where) {
  const ran = await runPage(APP, scenario(where, SECONDS), SECONDS + 150);
  try {
    assertEqual(ran.said.filter((m) => m.became_active), [], "the program became the active app");
    assert(ran.code === 0 && ran.done, "the page did not finish (exit " + ran.code + "): " + JSON.stringify(ran.said.slice(-4)) + " " + ran.stderr.slice(-300));
    assert(!ran.done.error, "the recording could not be made: " + ran.done.error + " " + JSON.stringify(ran.done.errors));
    assertEqual(ran.done.errors, [], "the page reported errors");
    assertEqual(ran.done.ended, "stopped", "the recorder did not close the file itself");
    const file = path.join(ran.dir, "video.mp4");
    assert(fs.existsSync(file), "no file was handed to the save window");
    const format = JSON.parse(execFileSync("ffprobe", ["-v", "error", "-show_format", "-show_streams", "-of", "json", file], { encoding: "utf8" }));
    const long = Number(format.format.duration);
    assertEqual(format.streams.map((s) => s.codec_name).sort(), ["aac", "h264"], "the file's streams");
    execFileSync("ffmpeg", ["-v", "fatal", "-i", file, "-f", "null", "-"], { stdio: ["ignore", "pipe", "pipe"] });
    const v = packets(file, "v:0");
    const a = packets(file, "a:0");
    const lastV = v[v.length - 1];
    const lastA = a[a.length - 1];
    const widest = v.slice(1).reduce((w, t, i) => Math.max(w, t - v[i]), 0);
    const facts = `${ran.done.name}: recorded ${ran.done.pressed.toFixed(1)} s, the file is ${long.toFixed(1)} s long, its picture runs to ${lastV.toFixed(1)} s in ${v.length} frames (no two more than ${widest.toFixed(2)} s apart), its sound to ${lastA.toFixed(1)} s; the sheet opened ${ran.done.sheetAfter.toFixed(2)} s after Stop and says "${ran.done.meta}"`;
    assert(ran.done.pressed >= SECONDS, "the recording was stopped early: " + facts);
    // The sheet names the recording's own length in whole seconds: never less than the 31 s that
    // passed while it ran, never more than the time since Start was pressed (both measured).
    const said = /^(\d+):(\d\d) /.exec(ran.done.meta);
    assert(said, "the sheet names no length. " + facts);
    const saidSeconds = Number(said[1]) * 60 + Number(said[2]);
    assert(saidSeconds >= SECONDS && saidSeconds <= ran.done.sinceGo, `the sheet's length is not the recording's (between ${SECONDS} s and ${ran.done.sinceGo.toFixed(1)} s). ` + facts);
    assert(long >= ran.done.pressed - 1.5, "the file is shorter than the recording. " + facts);
    assert(lastV >= ran.done.pressed - 1.5, "THE PICTURE FREEZES: it ends before the recording does. " + facts);
    assert(lastA >= ran.done.pressed - 1.5, "the sound ends before the recording does. " + facts);
    assert(widest <= 1, "the picture stands still for " + widest.toFixed(2) + " s in the middle of the file. " + facts);
    let moving = "";
    if (where === "home") {
      // the home screen's lights never stand still: the last second is not the picture of three seconds before
      const moved = differ(frameAt(file, lastV - 3.5), frameAt(file, lastV - 0.5));
      assert(moved >= 2, "the home screen's picture does not move in the file's last seconds (" + moved + " of 1,000 pixels differ). " + facts);
      moving = `; ${moved} of 1,000 pixels differ between its last second and three seconds before`;
    }
    return facts + moving;
  } finally {
    fs.rmSync(ran.dir, { recursive: true, force: true });
  }
}

async function main() {
  const run = createRun("a video of half a minute is whole in the Mac's own webview: the home screen's recorder and the one for every other page");

  await run.check("the home screen: a recording of 31 s with the microphone has its picture and its sound to the last second, and the picture keeps moving", async () => record("home"));

  await run.check("a chat: a recording of 31 s with the microphone has its picture and its sound to the last second", async () => record("chat"));

  process.exitCode = run.report() > 0 ? 1 : 0;
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

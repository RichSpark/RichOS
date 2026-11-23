// "MAKE A VIDEO", ANYWHERE IN THE APP (round 79).
//
// CEO, 2026-11-15: "Get someone on Fable to build round 79." His words for the feature: "When the
// user is not on the home page (and not on the splash screen), we need to add a "Make a video"
// button in the area shown here ... unlike on the home page, in this case there won't be an option
// for when recording stops. Because in that case, the user will always have to stop the recording.
// So, the idea here is: We let the user start a screen recording anywhere in the app. This will
// record whatever they want to record/show within the app." The design is
// `richos-hq/design/mockups/rounds/round-79`; the app's part is `video-anywhere.js`.
//
// THE RECORDER IS CHECKED BY ITS FILE, as the home screen's is (`home-video.js` in this
// directory). Each recording is saved with the sheet's own button; the browser stand-in's
// `home_video_save` keeps the bytes the Mac's save window would be handed, and the suite reads
// them back with ffprobe and ffmpeg. WHAT WAS RECORDED IS IN THE FILE: the recording goes from a
// chat into Settings, to the RichPass page and to the home screen; at each the suite takes a
// picture of the window, and the file's frame from that moment must be nearer to that picture
// than to the picture of any other screen.
//
// IT PLAYS NO SOUND AND OPENS NO MICROPHONE. The "microphone" is a tone made in the page that goes
// into the file only (`preset.homeVideoInput: "tone"`, the app's `RICHOS_VIDEO_INPUT_TONE`), and
// the sheet's player is muted for it; where the page asks for a real microphone (`getUserMedia`),
// the suite answers itself with a refusal. Headless: nothing is put on this Mac's screen.
//
// Run: node video-anywhere.js   (or `npm test` for every suite in this directory)

"use strict";

const { launchGuardedWebKit } = require("./lib/browser-watchdog");

const fs = require("fs");
const os = require("os");
const path = require("path");
const { execFileSync } = require("child_process");
const { loadPlaywright, createRun, assert, assertEqual, SEED_THEME, UI_DIR } = require("./lib/harness");
const png = require("./lib/png");

const APP = "file://" + path.join(UI_DIR, "index.html");
const TEAM = [
  { name: "Beth", role: "Bookkeeper" },
  { name: "Will", role: "Marketing Director" },
];
const NOW = new Date(2026, 10, 18, 10, 0, 0);
const KEY = "richos-home-growth";
const WEEK = Math.round(new Date(2026, 10, 16).getTime() / 86400000);
const SCRATCH = fs.mkdtempSync(path.join(os.tmpdir(), "richos-video-anywhere-"));

/// `getUserMedia` as the suite answers it: no microphone. The page's real one is never asked.
function refused() {
  window.__micAsked = 0;
  window.MediaDevices.prototype.getUserMedia = function () {
    window.__micAsked++;
    return Promise.reject(new Error("NotAllowedError"));
  };
}

/// The app with the curtain gone. `more.first` is a user on his first day (no team, no memory of
/// his own, so the home screen has no "Make a video" of its own); everybody else has a team and
/// 312 memories, all of them seen. `more.at === "home"` stays on the home screen; otherwise the
/// home screen is left for the conversation, as Talk to Rich leaves it.
async function open(browser, theme, more) {
  more = more || {};
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push("console: " + m.text());
  });
  page.__errors = errors;
  await page.clock.setFixedTime(NOW);
  await page.addInitScript(SEED_THEME, theme);
  await page.addInitScript(refused);
  const preset = Object.assign(more.first ? {} : { homeTeam: TEAM, homeField: { objects: 312, days: 67, lanes: 3 } }, more.preset || {});
  const store = more.first ? "" : JSON.stringify({ week: WEEK, stage: 4, look: { count: 312, at: NOW.getTime() - 3 * 3600 * 1000 } });
  await page.addInitScript(
    ([v, key, st]) => {
      window.__RICHOS_MOCK_PRESET__ = v;
      if (st) window.localStorage.setItem(key, st);
    },
    [preset, KEY, store]
  );
  await page.goto(APP);
  await page.waitForFunction("typeof window.RichHome === 'object' && typeof window.RichTimeline === 'object' && !!window.RichVideoAnywhere", null, { timeout: 60000 });
  await page.evaluate(() => window.RichSplash && window.RichSplash.yieldNow("acceptance-suite"));
  if (!more.first) {
    await page.evaluate(() => window.RichHome.startField());
    await page.waitForFunction("window.RichHome.state.field === 'live' && !!window.__loro", null, { timeout: 60000 });
    await page.waitForFunction("window.RichHomeGrowth.state().arrived === true", null, { timeout: 60000 });
  }
  await page.waitForFunction(() => !document.querySelector("#splash"), null, { timeout: 60000 });
  if (more.at !== "home") {
    await page.evaluate(() => window.RichHome.hide("suite"));
    await page.waitForFunction(() => !document.body.classList.contains("home-open") && !!document.querySelector("#video-anywhere"), null, { timeout: 60000 });
  }
  return page;
}

const st = (page) => page.evaluate(() => window.RichVideoAnywhere.state());
const until = (page, state) => page.waitForFunction((s) => window.RichVideoAnywhere.state().state === s, state, { timeout: 60000 });
/// The recording has run for `seconds` by its own clock on the red button ("0:03").
const recorded = (page, seconds) =>
  page.waitForFunction(
    (want) => {
      const t = document.querySelector("#video-anywhere-stop .rv-time").textContent.split(":");
      return t.length === 2 && Number(t[0]) * 60 + Number(t[1]) >= want;
    },
    seconds,
    { timeout: 60000 }
  );
/// The recorder has painted `frames` more frames of the screen as it stands now. Frames are at
/// least 33 ms apart, so twelve of them are more than the quarter of a second before a mark at
/// which the file is read, however slowly a busy host paints them.
async function painted(page, frames) {
  const from = (await st(page)).frames;
  await page.waitForFunction((want) => window.RichVideoAnywhere.state().frames >= want, from + frames, { timeout: 60000 });
}
const text = (page, sel) => page.evaluate((s) => (document.querySelector(s) ? document.querySelector(s).textContent.replace(/\s+/g, " ").trim() : null), sel);
const shown = (page, sel) =>
  page.evaluate((s) => {
    const e = document.querySelector(s);
    if (!e || e.hidden) return false;
    const cs = getComputedStyle(e);
    const r = e.getBoundingClientRect();
    return cs.display !== "none" && cs.visibility === "visible" && Number(cs.opacity) > 0.5 && r.width > 0 && r.height > 0;
  }, sel);
/// The same, given the time an opening takes (Settings rises over 0.2 s, the choices open over
/// 0.24 s, the sheet over 0.3 s).
const appears = (page, sel) =>
  page
    .waitForFunction(
      (s) => {
        const e = document.querySelector(s);
        if (!e || e.hidden) return false;
        const cs = getComputedStyle(e);
        const r = e.getBoundingClientRect();
        return cs.display !== "none" && cs.visibility === "visible" && Number(cs.opacity) > 0.99 && r.width > 0 && r.height > 0 && !e.getAnimations().some((a) => a.playState === "running");
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
const chosen = (page, label) => page.evaluate((l) => document.querySelector('.rv-seg[aria-label="' + l + '"] button[aria-pressed="true"]').textContent, label);
const choose = (page, label, choice) =>
  page.evaluate(
    ([l, c]) => {
      Array.from(document.querySelectorAll('.rv-seg[aria-label="' + l + '"] button'))
        .find((x) => x.textContent === c)
        .click();
    },
    [label, choice]
  );
/// Settings open and risen into place, then the button in its heading pressed: the choices.
async function openChoices(page) {
  await page.click("#set-btn");
  assert(await appears(page, "#set-menu"), "Settings did not open");
  await page.click("#video-anywhere");
  assert(await appears(page, "#video-anywhere-choices"), "the choices did not open");
}

// WCAG contrast from computed colors (the round's own `build/walk.js`). `backs` are the elements
// the thing stands on, lowest first; each one's background is composited over the theme's ground.
const CONTRAST = (rows) => {
  const cv = document.createElement("canvas");
  cv.width = cv.height = 1;
  const cx = cv.getContext("2d", { willReadFrequently: true });
  const rgba = (c) => {
    cx.clearRect(0, 0, 1, 1);
    cx.fillStyle = "#000";
    cx.fillStyle = c;
    cx.fillRect(0, 0, 1, 1);
    const d = cx.getImageData(0, 0, 1, 1).data;
    return [d[0], d[1], d[2], d[3] / 255];
  };
  const over = (f, b) => [0, 1, 2].map((i) => f[i] * f[3] + b[i] * (1 - f[3]));
  const lum = (c) => {
    const v = c.slice(0, 3).map((x) => {
      x /= 255;
      return x <= 0.03928 ? x / 12.92 : Math.pow((x + 0.055) / 1.055, 2.4);
    });
    return 0.2126 * v[0] + 0.7152 * v[1] + 0.0722 * v[2];
  };
  const ratio = (a, b) => {
    const x = lum(a);
    const y = lum(b);
    return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
  };
  const ground = rgba(getComputedStyle(document.documentElement).getPropertyValue("--ground").trim()).slice(0, 3);
  const out = [];
  for (const [name, sel, backs, prop, need] of rows) {
    const el = document.querySelector(sel);
    if (!el) {
      out.push({ name, ratio: 0, needs: need || 4.5, missing: sel });
      continue;
    }
    const s = getComputedStyle(el);
    let bg = ground;
    let missing = null;
    for (const b of backs || []) {
      const be = document.querySelector(b);
      if (!be) {
        missing = b;
        break;
      }
      bg = over(rgba(getComputedStyle(be).backgroundColor), bg);
    }
    if (missing) {
      out.push({ name, ratio: 0, needs: need || 4.5, missing });
      continue;
    }
    // a font size under 16px is not text meant to be easily read here: nothing of the recorder's is
    out.push({ name, ratio: Math.round(ratio(over(rgba(s[prop || "color"]), bg), bg) * 100) / 100, needs: need || 4.5, px: prop ? null : parseFloat(s.fontSize) });
  }
  return out;
};
/// Every row clears its floor (and every text row is 16px or larger); the nearest to its floor, for the report.
async function clears(page, theme, rows) {
  // a choice changes color over 0.2 s: each is measured as it stands once it has changed
  await page.addStyleTag({ content: ".rv-skip, .rv-skip * { transition: none !important; }" });
  const got = await page.evaluate(CONTRAST, rows);
  assertEqual(got.filter((r) => r.missing || !(r.ratio >= r.needs) || (r.px !== null && r.px < 16)), [], theme + ": below its contrast floor, under 16px, or not found");
  const worst = got.reduce((a, b) => (b.ratio / b.needs < a.ratio / a.needs ? b : a));
  return `${got.length} measured, the nearest to its floor "${worst.name}" ${worst.ratio}:1 (needs ${worst.needs})`;
}

/// Press "Save video" and read back the file the save window was handed.
async function saved(page, tag) {
  const before = await page.evaluate(() => window.__RICHOS_MOCK__.homeVideoSaves().length);
  await page.click(".rv-sheet .rv-main");
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
  const out = file.at + "-" + seconds.toFixed(2) + ".png";
  execFileSync("ffmpeg", ["-v", "error", "-y", "-ss", seconds.toFixed(2), "-i", file.at, "-frames:v", "1", "-pix_fmt", "rgb24", out]);
  return png.decode(fs.readFileSync(out));
}
/// The window as it stands, decoded (a picture taken by the browser, never put on a screen).
async function windowOf(page) {
  return png.decode(await page.screenshot({ fullPage: false }));
}
/// How far two pictures of the same size are apart: the mean difference of a channel, over every
/// eighth pixel of every eighth row.
function apart(a, b) {
  let sum = 0;
  let n = 0;
  for (let y = 4; y < a.height; y += 8) {
    for (let x = 4; x < a.width; x += 8) {
      const i = (y * a.width + x) * a.channels;
      const j = (y * b.width + x) * b.channels;
      sum += Math.abs(a.data[i] - b.data[j]) + Math.abs(a.data[i + 1] - b.data[j + 1]) + Math.abs(a.data[i + 2] - b.data[j + 2]);
      n += 3;
    }
  }
  return sum / n;
}
/// Is the recorder's arrow in a decoded frame with its tip at x,y (`home-video.js`, `paintPointer`:
/// near-black with a white edge; its left edge runs straight down from the tip and its inside is
/// 2.4px to the right of it)? A light page is bright in both places and a dark one dark in both.
function arrowAt(img, x, y) {
  const mean = (px, py) => {
    const i = (py * img.width + px) * img.channels;
    return (img.data[i] + img.data[i + 1] + img.data[i + 2]) / 3;
  };
  let edge = 255;
  let inside = 0;
  for (let dy = 4; dy <= 16; dy++) edge = Math.min(edge, Math.max(mean(x - 1, y + dy), mean(x, y + dy)));
  for (let dy = 7; dy <= 13; dy++) inside = Math.max(inside, mean(x + 3, y + dy));
  return { edge: Math.round(edge), inside: Math.round(inside), there: edge > 130 && inside < 80 };
}
/// How many pixels in a box are a given color, as H.264 keeps it.
function colorIn(img, box, rgb) {
  let n = 0;
  for (let y = box.top; y < box.bottom; y++) {
    for (let x = box.left; x < box.right; x++) {
      const i = (y * img.width + x) * img.channels;
      if (Math.abs(img.data[i] - rgb[0]) < 40 && Math.abs(img.data[i + 1] - rgb[1]) < 40 && Math.abs(img.data[i + 2] - rgb[2]) < 40) n++;
    }
  }
  return n;
}

async function main() {
  const run = createRun("Make a video, anywhere in the app: round 79, the button in the heading of Settings that records RichOS on every screen until he stops it");
  const { webkit } = loadPlaywright();
  const browser = await launchGuardedWebKit(webkit, run);

  await run.check("the button stands in the heading of Settings, at its right end, on every screen that is not the home screen; nothing of the recorder is on a screen until Settings is open", async () => {
    const page = await open(browser, "dark");
    assertEqual([await shown(page, "#video-anywhere"), await shown(page, "#video-anywhere-stop"), await shown(page, "#video-anywhere-choices")], [false, false, false], "a piece of the recorder is on the screen with Settings closed");
    await page.click("#set-btn");
    assert(await appears(page, "#set-menu"), "Settings did not open");
    assert(await shown(page, "#video-anywhere"), "Make a video is not in Settings on a chat");
    const where = await page.evaluate(() => {
      const m = document.querySelector("#video-anywhere");
      const b = m.getBoundingClientRect();
      const head = document.querySelector("#set-menu .sm-head");
      const h = head.getBoundingClientRect();
      const cs = getComputedStyle(head);
      const title = document.querySelector("#set-menu-title").getBoundingClientRect().height;
      return {
        parent: m.parentNode.className,
        inHead: b.top >= h.top && b.bottom <= h.bottom,
        fromRight: Math.round(h.right - b.right),
        height: Math.round(b.height),
        // the heading is as tall as it is without the button: its line, its padding, its rule
        taller: Math.round((h.height - (title + parseFloat(cs.paddingTop) + parseFloat(cs.paddingBottom) + parseFloat(cs.borderBottomWidth))) * 10) / 10,
        word: m.querySelector(".w").textContent,
        name: m.getAttribute("aria-label"),
        light: getComputedStyle(m.querySelector(".rv-light")).display,
        icon: getComputedStyle(m.querySelector(".rv-ico")).color,
        gold: getComputedStyle(document.documentElement).getPropertyValue("--gold-text").trim(),
        buttons: document.querySelectorAll(".rv-make").length,
      };
    });
    assertEqual([where.parent, where.inHead, where.fromRight, where.height], ["sm-head", true, 22, 36], "the button is not in the heading of Settings at its right end");
    assert(Math.abs(where.taller) < 0.6, "the button makes the heading of Settings " + where.taller + "px taller");
    assertEqual([where.word, where.name, where.light, where.buttons], ["Make a video", "Make a video", "none", 1], "one button that says Make a video, with nothing red on it");
    assertEqual(await page.evaluate((c) => {
      const p = document.createElement("i");
      p.style.color = c;
      document.body.appendChild(p);
      const v = getComputedStyle(p).color;
      p.remove();
      return v;
    }, where.gold), where.icon, "the video icon is not the gold of the theme");
    // every screen that is not the home screen: another chat, a tool's page
    await page.keyboard.press("Escape");
    await page.click("text=Acme deal");
    await page.click("#set-btn");
    assert((await appears(page, "#set-menu")) && (await shown(page, "#video-anywhere")), "Make a video is not in Settings on another chat");
    await page.keyboard.press("Escape");
    await page.click("#nr-pass");
    await page.click("#set-btn");
    assert((await appears(page, "#set-menu")) && (await shown(page, "#video-anywhere")), "Make a video is not in Settings on the RichPass page");
    await page.keyboard.press("Escape");
    // the home screen keeps its own, and this one is not there on it
    await page.click("#nr-home");
    await page.waitForFunction(() => window.RichHome.isOpen(), null, { timeout: 60000 });
    assert(await shown(page, "#home-video"), "the home screen lost its own Make a video");
    await page.click("#set-btn");
    assert(await appears(page, "#set-menu"), "Settings did not open on the home screen");
    assertEqual(await shown(page, "#video-anywhere"), false, "Make a video is in Settings on the home screen, which has its own");
    assertEqual(page.__errors, [], "the page reported errors");
    await page.close();
    return `in .sm-head, ${where.fromRight}px from its right end, ${where.height}px tall, the heading no taller; on a chat, another chat and the RichPass page; not on the home screen`;
  });

  await run.check("pressed, it opens two choices and no choice for when the recording stops; as first opened the microphone is off and the pointer is shown; they close by Escape, a press outside, the button and Settings closing", async () => {
    const page = await open(browser, "dark");
    await openChoices(page);
    assertEqual(await page.evaluate(() => document.querySelector("#video-anywhere").getAttribute("aria-expanded")), "true", "the button does not say its choices are open");
    assertEqual(await page.evaluate(() => Array.from(document.querySelectorAll("#video-anywhere-choices .rv-row .lab span:not(.rv-level)")).map((e) => e.textContent)), ["Microphone", "Mouse pointer"], "the two rows");
    assertEqual(await page.evaluate(() => Array.from(document.querySelectorAll("#video-anywhere-choices .rv-seg button")).map((e) => e.textContent)), ["Off", "On", "Shown", "Hidden"], "the four choices");
    assertEqual(await page.evaluate(() => /Recording stops|By itself|When I stop it/.test(document.querySelector("#video-anywhere-choices").textContent)), false, "there is a choice for when the recording stops");
    assertEqual([await chosen(page, "Microphone"), await chosen(page, "Mouse pointer")], ["Off", "Shown"], "as first opened");
    assertEqual(await text(page, "#video-anywhere-choices .says"), "RichOS is recorded while you use it, on every screen you go to, until you press Stop.", "the sentence under the choices");
    const seat = await page.evaluate(() => {
      const b = document.querySelector("#video-anywhere").getBoundingClientRect();
      const p = document.querySelector("#video-anywhere-choices").getBoundingClientRect();
      return [Math.round(p.top - b.bottom), Math.round(p.right - b.right), Math.round(p.width)];
    });
    assertEqual(seat, [8, 0, 430], "the choices do not hang 8px under their button, flush with its right end, 430px wide");
    assertEqual(await page.evaluate(() => document.activeElement.textContent), "Start recording", "the keyboard is not on Start recording");
    // Closing the choices without recording restores the last recorded settings.
    await choose(page, "Mouse pointer", "Hidden");
    await page.keyboard.press("Escape");
    assertEqual([(await st(page)).state, await shown(page, "#video-anywhere-choices"), await shown(page, "#set-menu")], ["idle", false, true], "Escape did not close the choices alone");
    assertEqual(await page.evaluate(() => document.activeElement.id), "video-anywhere", "the keyboard is not back on the button");
    await page.click("#video-anywhere");
    assertEqual(await chosen(page, "Mouse pointer"), "Shown", "an unrecorded pointer choice became the default");
    await page.mouse.click(520, 430); // a press on Settings, outside the choices
    assertEqual([(await st(page)).state, await shown(page, "#set-menu")], ["idle", true], "a press outside did not close the choices and leave Settings open");
    await page.click("#video-anywhere");
    await page.click("#video-anywhere");
    assertEqual((await st(page)).state, "idle", "the button did not close its own choices");
    await page.click("#video-anywhere");
    await page.keyboard.press("Escape");
    await page.keyboard.press("Escape");
    assertEqual(await shown(page, "#set-menu"), false, "a second Escape did not close Settings");
    await openChoices(page);
    await page.mouse.click(26, 420); // a press outside Settings altogether
    await page.waitForFunction(() => window.RichVideoAnywhere.state().state === "idle", null, { timeout: 60000 }).catch(() => {});
    assertEqual([(await st(page)).state, await shown(page, "#set-menu"), await shown(page, "#video-anywhere-choices")], ["idle", false, false], "Settings closing did not take the choices with it");
    assertEqual(await page.evaluate(() => window.__micAsked), 0, "the microphone was asked for and he never switched it on");
    assertEqual(page.__errors, [], "the page reported errors");
    await page.close();
    return "Microphone (Off, On) and Mouse pointer (Shown, Hidden), 430px wide, 8px under the button; an abandoned Hidden choice reset; closed four ways";
  });

  await run.check("a recording with the microphone and the pointer goes on from a chat into Settings, to the RichPass page and to the home screen, and is stopped there: every one of those screens is in the saved file, with sound and the pointer and without the recorder's own button", async () => {
    // light, so the screens differ as much in the file as they do in the window (the home screen is always dark)
    const page = await open(browser, "light", { preset: { homeVideoInput: "tone" } });
    await page.mouse.move(760, 520);
    await openChoices(page);
    await choose(page, "Microphone", "On");
    await page.waitForFunction(() => window.RichVideoAnywhere.state().mic === "on", null, { timeout: 60000 });
    assertEqual(await page.evaluate(() => window.__micAsked), 0, "a test run's made-up microphone asked for the real one");
    assert(await shown(page, "#video-anywhere-choices .rv-row .rv-level"), "the bars that move with his voice are not beside Microphone");
    await page.evaluate(() => {
      const start = MediaRecorder.prototype.start;
      MediaRecorder.prototype.start = function (...args) {
        const result = start.apply(this, args);
        window.__mediaRecorderStartedAt = performance.now();
        return result;
      };
    });
    await page.click("#video-anywhere-choices .rv-go");
    await until(page, "count");
    // Settings closes, so the video starts on the app; the count; Cancel beside the settings button
    assertEqual([await shown(page, "#set-menu"), await shown(page, ".rv-count"), await text(page, ".rv-count .n"), await text(page, ".rv-count .plain"), await text(page, ".rv-count .voice")], [false, true, "3", "Recording starts", "Your microphone is on"], "the count");
    assertEqual([await shown(page, "#video-anywhere-stop"), await text(page, "#video-anywhere-stop .w")], [true, "Cancel"], "the button beside the settings button during the count");
    const t0 = Date.now();
    await until(page, "rec");
    const counted = (Date.now() - t0) / 1000;
    assert(counted > 1.6, "the count took " + counted.toFixed(1) + " s, not three steps of 0.8");
    // WHERE IN THE FILE EACH SCREEN IS. A mark is taken after the recorder has painted the screen
    // twelve times or more, with the window's own picture; the file's frame is read a quarter of a
    // second before it.
    const fileTime = () => page.evaluate(() => (performance.now() - window.__mediaRecorderStartedAt) / 1000);
    const marks = [];
    const mark = async (tag) => {
      const at = await fileTime() - 0.25;
      marks.push({ tag, at, window: await windowOf(page) });
    };
    await page.mouse.move(620, 300, { steps: 12 });
    await recorded(page, 1);
    assertEqual([await text(page, "#video-anywhere-stop .w"), await page.evaluate(() => document.querySelector("#video-anywhere-stop").getAttribute("aria-label")), /^\d:\d\d$/.test(await text(page, "#video-anywhere-stop .rv-time"))], ["Stop", "Stop recording", true], "the red button while it records");
    const lane = await page.evaluate(() => {
      const p = document.querySelector("#video-anywhere-stop").getBoundingClientRect();
      const s = document.querySelector("#set-btn").getBoundingClientRect();
      const o = document.querySelector("#out-top");
      const ob = o && !o.hidden ? o.getBoundingClientRect() : null;
      const red = getComputedStyle(document.documentElement).getPropertyValue("--rv-red").trim();
      const m = document.querySelector("#video-anywhere-stop");
      return { gap: Math.round(s.left - p.right), level: Math.abs(p.top + p.height / 2 - (s.top + s.height / 2)) < 1.5, overOutput: !!ob && ob.right > p.left - 2 && ob.left < p.right, red, edge: getComputedStyle(m).borderTopColor, light: getComputedStyle(m.querySelector(".rv-light")).display };
    });
    assertEqual([lane.gap, lane.level, lane.overOutput, lane.red, lane.edge, lane.light !== "none"], [8, true, false, "#b93426", "rgb(185, 52, 38)", true], "Stop does not stand red beside the settings button, level with it, clear of the Output button");
    assert(await shown(page, "#video-anywhere-stop .rv-level"), "the bars are not on the button while the microphone records");
    const stop = await rect(page, "#video-anywhere-stop");
    await mark("a chat");
    // TEXT HE HIGHLIGHTS IS IN THE FILE (CEO 2026-11-15: "I would expect the screen recording to
    // record the text highlighting as well"): a line of the conversation is selected, as a drag
    // of the mouse over it selects it. Keep it selected until the actual recording canvas
    // contains the highlight, then leave enough captured frames for the file's timestamp sample.
    const lit = await page.evaluate(() => {
      const walker = document.createTreeWalker(document.querySelector("#app"), NodeFilter.SHOW_TEXT);
      for (let n = walker.nextNode(); n; n = walker.nextNode()) {
        if (n.textContent.trim().length < 30 || n.parentElement.closest(".rv-skip, button, nav, aside")) continue;
        const cs = getComputedStyle(n.parentElement);
        if (cs.userSelect === "none" || cs.webkitUserSelect === "none" || cs.visibility !== "visible") continue;
        const r = document.createRange();
        r.selectNodeContents(n);
        const b = r.getClientRects()[0];
        if (!b || b.width < 200 || b.top < 80 || b.bottom > innerHeight - 80 || b.left < 300) continue;
        const sel = window.getSelection();
        sel.removeAllRanges();
        sel.addRange(r);
        const probe = document.createElement("i");
        probe.style.cssText = "display:none;background-color:Highlight";
        document.body.appendChild(probe);
        // the Mac's highlight color may be see-through: as it lies on what is under this line
        const c = getComputedStyle(probe).backgroundColor.match(/[\d.]+/g).map(Number);
        probe.remove();
        const alpha = c.length > 3 ? c[3] : 1;
        let under = [255, 255, 255];
        for (let e = n.parentElement; e; e = e.parentElement) {
          const bg = getComputedStyle(e).backgroundColor.match(/[\d.]+/g).map(Number);
          if (bg.length < 4 || bg[3] === 1) {
            under = bg.slice(0, 3);
            break;
          }
        }
        const color = [0, 1, 2].map((i) => Math.round(c[i] * alpha + under[i] * (1 - alpha)));
        return { text: n.textContent.trim().slice(0, 40), color, raw: c, box: { left: Math.ceil(b.left), top: Math.ceil(b.top), right: Math.floor(b.right), bottom: Math.floor(b.bottom) } };
      }
      return null;
    });
    assert(lit, "the conversation has no line of text to highlight");
    await page.waitForFunction(({ box, color }) => {
      const canvas = document.querySelector(".rv-canvas");
      if (!canvas) return false;
      const width = box.right - box.left;
      const height = box.bottom - box.top;
      const pixels = canvas.getContext("2d").getImageData(box.left, box.top, width, height).data;
      let tinted = 0;
      for (let i = 0; i < pixels.length; i += 4) {
        if (Math.abs(pixels[i] - color[0]) < 40 &&
            Math.abs(pixels[i + 1] - color[1]) < 40 &&
            Math.abs(pixels[i + 2] - color[2]) < 40) tinted++;
      }
      return tinted > width * height * 0.3;
    }, lit, { timeout: 60000 }).catch((error) => {
      if (error.name !== "TimeoutError") throw error;
      throw new Error(`The recorder never painted the selection on "${lit.text}"`, { cause: error });
    });
    await painted(page, 12);
    lit.at = await fileTime() - 0.25;
    await page.evaluate(() => window.getSelection().removeAllRanges());
    // Settings, while it records: the button in its heading is the same red Stop
    await page.click("#set-btn");
    assert(await appears(page, "#set-menu"), "Settings did not open during a recording");
    await page.mouse.move(560, 330, { steps: 12 });
    assertEqual([await text(page, "#video-anywhere .w"), await page.evaluate(() => document.querySelector("#video-anywhere").classList.contains("going")), await text(page, "#video-anywhere .rv-time") === await text(page, "#video-anywhere-stop .rv-time")], ["Stop", true, true], "in Settings the button is not the same red Stop with the time");
    await painted(page, 12);
    await mark("Settings");
    // ESCAPE DOES NOT STOP A RECORDING HERE: it closes Settings and the recording goes on
    await page.keyboard.press("Escape");
    assertEqual([(await st(page)).state, await shown(page, "#set-menu")], ["rec", false], "Escape did not close Settings alone");
    await page.keyboard.press("Escape");
    assertEqual((await st(page)).state, "rec", "Escape stopped a recording");
    await page.click("#nr-pass");
    await page.mouse.move(820, 380, { steps: 12 });
    await painted(page, 12);
    assertEqual([(await st(page)).state, await shown(page, "#video-anywhere-stop")], ["rec", true], "the RichPass page ended the recording or has no Stop");
    await mark("the RichPass page");
    await page.click("#nr-home");
    await page.waitForFunction(() => window.RichHome.isOpen(), null, { timeout: 60000 });
    await page.mouse.move(700, 600, { steps: 12 });
    await painted(page, 30); // the home screen has faded in, and stands in the file as above
    // on the home screen the red Stop stands where the home screen's own button stands
    assertEqual([(await st(page)).state, await shown(page, "#video-anywhere-stop"), await shown(page, "#home-video")], ["rec", true, false], "on the home screen: still recording, Stop there, the home screen's own button out of its way");
    await mark("the home screen");
    const cost = await st(page);
    await page.click("#video-anywhere-stop");
    await until(page, "sheet");
    assert(await appears(page, ".rv-sheet"), "the sheet did not open");
    assertEqual(await text(page, ".rv-sheet h2"), "Your video", "the sheet's heading");
    const meta = await text(page, ".rv-sheet .meta");
    assert(/^0:\d\d · with your voice · with the mouse pointer · 1,440 × 900 · \d+\.\d MB$/.test(meta), "the sheet's line: " + meta);
    assertEqual(await page.evaluate(() => Array.from(document.querySelectorAll(".rv-sheet .row button")).filter((b) => !b.hidden).map((b) => b.textContent)), ["Save video", "Record again", "Discard"], "the sheet's buttons");
    assertEqual(await page.evaluate(() => {
      const v = document.querySelector(".rv-sheet video");
      return { muted: v.muted, src: v.src.slice(0, 5), hidden: v.hidden };
    }), { muted: true, src: "blob:", hidden: false }, "the player (muted for the made-up microphone)");
    assertEqual([(await st(page)).mic, (await st(page)).ended], ["off", "stopped"], "the microphone was kept after the file was whole, or the recorder did not close the file itself");
    // THE FILE
    const file = await saved(page, "anywhere");
    assertEqual(file.name, "RichOS-2026-11-18-at-10.00.00.mp4", "the name of the file says when it was made");
    assertEqual([file.video && file.video.codec, file.video && file.video.width, file.video && file.video.height, file.audio], ["h264", 1440, 900, "aac"], "the file's streams");
    const said = Number(meta.slice(2, 4));
    const last = marks[marks.length - 1].at;
    assert(Math.abs(file.seconds - said) <= 1.5 && file.seconds >= last, `the file is ${file.seconds.toFixed(1)} s long, the sheet says 0:${meta.slice(2, 4)} and the last screen was marked at ${last.toFixed(1)} s`);
    await page.waitForFunction(() => !document.querySelector(".rv-sheet .saved").hidden, null, { timeout: 60000 });
    assertEqual(await text(page, ".rv-sheet .saved"), "Saved to Desktop as RichOS-2026-11-18-at-10.00.00.mp4", "where it says the file went");
    assertEqual(await page.evaluate(() => Array.from(document.querySelectorAll(".rv-sheet .row button")).filter((b) => !b.hidden).map((b) => b.textContent)), ["Record again", "Done"], "the sheet's buttons after saving");
    // EVERY SCREEN HE WENT TO IS IN THE FILE, WHERE HE WAS ON IT: each mark's frame is nearer to
    // the window's own picture at that mark than to the picture at any other.
    const table = [];
    for (const m of marks) {
      const frame = frameOf(file, m.at);
      const far = marks.map((o) => apart(frame, o.window));
      const nearest = marks[far.indexOf(Math.min(...far))].tag;
      table.push(`${m.tag} at ${m.at.toFixed(1)} s: ${far.map((d) => d.toFixed(1)).join(" / ")}`);
      assertEqual(nearest, m.tag, `the file's frame at ${m.at.toFixed(1)} s is nearest to the window on ${nearest} (${far.map((d) => d.toFixed(1)).join(" / ")}), and he was on ${m.tag}`);
      m.frame = frame;
    }
    // the pointer is in the file where the mouse was (on a chat, at 620,300), and the recorder's own
    // red button is not, where the window has it
    const arrow = arrowAt(marks[0].frame, 620, 300);
    assert(arrow.there, "no mouse pointer in the file at 620,300: " + JSON.stringify(arrow));
    const red = [colorIn(marks[0].window, stop, [185, 52, 38]), colorIn(marks[0].frame, stop, [185, 52, 38])];
    assert(red[0] > 60 && red[1] < 6, `the recorder's own button: ${red[0]} red pixels in the window and ${red[1]} in the file, where there must be none`);
    // the highlight is in the file behind the line he selected, while it was selected and not before
    const area = (lit.box.right - lit.box.left) * (lit.box.bottom - lit.box.top);
    const tinted = [colorIn(marks[0].frame, lit.box, lit.color), colorIn(frameOf(file, lit.at), lit.box, lit.color)];
    assert(tinted[0] < area * 0.02 && tinted[1] > area * 0.3, `the highlight on "${lit.text}": ${tinted[0]} of ${area} pixels in its color before he selected it and ${tinted[1]} while it was selected (the color ${JSON.stringify(lit.raw)}, on the page ${JSON.stringify(lit.color)})`);
    await page.click(".rv-sheet .row .rv-plain:last-child");
    assertEqual([(await st(page)).state, await shown(page, ".rv-sheet"), await shown(page, "#video-anywhere-stop"), await shown(page, "#home-video")], ["idle", false, false, true], "Done did not put everything back");
    assertEqual(page.__errors, [], "the page reported errors");
    await page.close();
    return `count ${counted.toFixed(1)} s; ${file.name}: ${file.video.codec} ${file.video.width}×${file.video.height} + ${file.audio}, ${file.seconds.toFixed(1)} s, ${(file.size / 1048576).toFixed(1)} MB; each frame against the window on ${marks.map((m) => m.tag).join(" / ")}: ${table.join("; ")}; pointer in the frame (edge ${arrow.edge}, inside ${arrow.inside}); the red button ${red[0]} px in the window, ${red[1]} in the file; the highlight on "${lit.text}" ${tinted[0]} px before, ${tinted[1]} of ${area} while selected; a frame of the home screen cost ${cost.cost.toFixed(1)} ms (${cost.nodes} pieces)`;
  });

  await run.check("no microphone is said plainly and records without sound; Enter starts; Escape and Cancel call the count off; every pane in the top right corner is clear of Stop; the save window's Cancel and a folder that cannot be written; Record again opens Settings with the choices as he left them; Escape discards", async () => {
    const page = await open(browser, "dark", { preset: { homeVideoSave: "cancel" } });
    await page.click('.nav-thread[data-thread-id="hiring"]'); // a chat with files in its Output panel and a worker to look at
    await page.mouse.move(700, 400);
    await openChoices(page);
    await choose(page, "Microphone", "On");
    await page.waitForFunction(() => window.RichVideoAnywhere.state().mic === "denied", null, { timeout: 60000 }).catch(() => {});
    assertEqual([(await st(page)).mic, await page.evaluate(() => window.__micAsked), await chosen(page, "Microphone")], ["denied", 1, "Off"], "a microphone that was refused");
    assertEqual([await shown(page, "#video-anywhere-choices .rv-mic-note"), await text(page, "#video-anywhere-choices .rv-mic-note")], [true, "This window was not given a microphone. The video will have no sound."], "what it says with no microphone");
    await choose(page, "Mouse pointer", "Hidden");
    // Enter on Start recording starts the count; Escape calls it off
    await page.focus("#video-anywhere-choices .rv-go");
    await page.keyboard.press("Enter");
    await until(page, "count");
    assertEqual(await shown(page, ".rv-count .voice"), false, "the count says the microphone is on when it is not");
    await page.keyboard.press("Escape");
    assertEqual([(await st(page)).state, await shown(page, ".rv-count"), await shown(page, "#video-anywhere-stop")], ["idle", false, false], "Escape did not call the count off");
    // load-bound: the count's timers were cleared when Escape was taken; the wait is longer than the count (2.4 s) only so a timer that leaked would have fired, and a slow host cannot make a correct build fail it
    await page.waitForTimeout(2800);
    assertEqual((await st(page)).state, "idle", "a count that was called off started a recording");
    // Cancel on the button beside the settings button: Settings again, with the choices open as he left them
    await openChoices(page);
    assertEqual(await chosen(page, "Mouse pointer"), "Shown", "Escape kept a choice from a cancelled recording");
    await choose(page, "Mouse pointer", "Hidden");
    await page.click("#video-anywhere-choices .rv-go");
    await until(page, "count");
    await page.click("#video-anywhere-stop");
    await until(page, "choosing");
    assertEqual([await appears(page, "#set-menu"), await appears(page, "#video-anywhere-choices"), await shown(page, ".rv-count")], [true, true, false], "Cancel did not go back to Settings with the choices open");
    assertEqual(await chosen(page, "Mouse pointer"), "Hidden", "Cancel did not keep the draft open for revision");
    await page.click("#video-anywhere-choices .rv-go");
    await until(page, "rec");
    await page.mouse.move(700, 400, { steps: 12 });
    await recorded(page, 2);
    // EVERY PANE THAT REACHES THE TOP RIGHT CORNER, OPENED WHILE IT RECORDS, is clear of the red
    // Stop, which stands above them all: the conversation's heading, the Output panel, the worker
    // details. A press on a pane's close button closes the pane and the recording goes on.
    const clearOfStop = (sel) =>
      page.evaluate((s) => {
        const c = document.querySelector(s).getBoundingClientRect();
        const stop = document.querySelector("#video-anywhere-stop").getBoundingClientRect();
        const at = document.elementFromPoint(c.left + c.width / 2, c.top + c.height / 2);
        return { underStop: !(c.right <= stop.left || c.left >= stop.right || c.bottom <= stop.top || c.top >= stop.bottom), pressed: !!at && !!at.closest(s), gap: Math.round(stop.left - c.right) };
      }, sel);
    const gaps = [];
    const top = await clearOfStop("#out-top");
    assertEqual([top.underStop, top.pressed], [false, true], "the conversation's Output button is under the red Stop: " + JSON.stringify(top));
    gaps.push("the conversation's Output button " + top.gap + "px");
    await page.click("#out-top");
    assert(await appears(page, "#outpanel"), "the Output panel did not open during a recording");
    const close = await clearOfStop("#op-close");
    assertEqual([close.underStop, close.pressed], [false, true], "the Output panel's close button is under the red Stop: " + JSON.stringify(close));
    gaps.push("the Output panel's close " + close.gap + "px");
    await page.click("#op-close");
    assertEqual([(await st(page)).state, await shown(page, "#outpanel")], ["rec", false], "close on the Output panel did not close it alone");
    await page.click(".tl-duration-btn");
    await page.click(".tl-chip");
    assert(await appears(page, "#inspector"), "the worker details did not open during a recording");
    const details = await clearOfStop("#inspector-close");
    assertEqual([details.underStop, details.pressed], [false, true], "the worker details' close button is under the red Stop: " + JSON.stringify(details));
    gaps.push("the worker details' close " + details.gap + "px");
    await page.click("#inspector-close");
    assertEqual([(await st(page)).state, await shown(page, "#inspector")], ["rec", false], "close on the worker details did not close them alone");
    // and every heading that adds the lane is counted in the shipped stylesheets, so a pane added to
    // that corner sends its author here to open it under a recording too: the conversation's
    // heading twice (its own rule and the one that adds the live lane), the worker details', the
    // Output panel's
    const lanes = [];
    for (const file of fs.readdirSync(UI_DIR).filter((f) => f.endsWith(".css"))) {
      const lines = fs.readFileSync(path.join(UI_DIR, file), "utf8").split("\n");
      lines.forEach((line, i) => {
        if (/^\s*padding-right:.*var\(--chrome-lane\b/.test(line)) lanes.push(file + ":" + (i + 1));
      });
    }
    assertEqual(lanes.length, 4, "the headings that keep the top right lane clear are not the three this check opens: " + lanes.join(", "));
    await page.mouse.move(700, 400, { steps: 12 });
    await page.click("#video-anywhere-stop");
    await until(page, "sheet");
    assert(await appears(page, ".rv-sheet"), "the sheet did not open");
    const meta = await text(page, ".rv-sheet .meta");
    assert(/^0:0\d · no sound · no mouse pointer · 1,440 × 900 · \d+\.\d MB$/.test(meta), "the sheet's line: " + meta);
    // his Cancel in the save window changes nothing on the sheet
    await page.click(".rv-sheet .rv-main");
    await page.waitForFunction(() => window.__RICHOS_MOCK__.homeVideoSaves().length === 1 && !document.querySelector(".rv-sheet .rv-main").disabled, null, { timeout: 60000 });
    assertEqual([await shown(page, ".rv-sheet .saved"), await shown(page, ".rv-sheet .rv-main"), await text(page, ".rv-sheet .row .rv-plain:last-child")], [false, true, "Discard"], "his Cancel changed the sheet");
    await page.evaluate(() => (window.__RICHOS_MOCK_PRESET__.homeVideoSave = "fail"));
    await page.click(".rv-sheet .rv-main");
    await page.waitForFunction(() => !document.querySelector(".rv-sheet .saved").hidden, null, { timeout: 60000 });
    assertEqual([await text(page, ".rv-sheet .saved"), await shown(page, ".rv-sheet .rv-main")], ["The video could not be saved there. Choose another folder.", true], "a folder that cannot be written");
    await page.evaluate(() => (window.__RICHOS_MOCK_PRESET__.homeVideoSave = "save"));
    const file = await saved(page, "silent");
    assertEqual([file.video && file.video.codec, file.video && file.video.width, file.audio], ["h264", 1440, null], "a video made without a microphone");
    assert(file.seconds >= 2, "the file is " + file.seconds.toFixed(1) + " s long");
    const arrow = arrowAt(frameOf(file, 1), 700, 400);
    assertEqual(arrow.there, false, "a mouse pointer is in a file made with the pointer hidden: " + JSON.stringify(arrow));
    // Record again: Settings, with the choices open as he left them
    await page.click(".rv-sheet .row .rv-plain");
    await until(page, "choosing");
    assertEqual([await shown(page, ".rv-sheet"), await appears(page, "#set-menu"), await appears(page, "#video-anywhere-choices"), await chosen(page, "Mouse pointer")], [false, true, true, "Hidden"], "Record again did not open Settings with the choices as he left them");
    await page.click("#video-anywhere-choices .rv-go");
    await until(page, "rec");
    await recorded(page, 1);
    await page.click("#video-anywhere-stop");
    await until(page, "sheet");
    const handed = await page.evaluate(() => window.__RICHOS_MOCK__.homeVideoSaves().length);
    await page.keyboard.press("Escape");
    assertEqual([(await st(page)).state, await shown(page, ".rv-sheet"), await page.evaluate(() => window.__RICHOS_MOCK__.homeVideoSaves().length)], ["idle", false, handed], "Escape did not discard the video");
    assertEqual(page.__errors, [], "the page reported errors");
    await page.close();
    return `no microphone: the sentence, Off, no sound; ${file.name}: ${file.video.codec}, no sound, ${file.seconds.toFixed(1)} s, no pointer in a frame (edge ${arrow.edge}, inside ${arrow.inside}); the count called off by Escape and by Cancel; clear of Stop: ${gaps.join(", ")}; Cancel and a failed save said; Record again kept Hidden; Escape discarded`;
  });

  await run.check("a recording stopped on the home screen: Space on the sheet's player pauses and plays it and the home screen stays, and Record again opens the home screen's own choices; on a home screen with no button of its own it opens these choices", async () => {
    let page = await open(browser, "dark");
    await openChoices(page);
    await page.click("#video-anywhere-choices .rv-go");
    await until(page, "rec");
    await page.click("#nr-home");
    await page.waitForFunction(() => window.RichHome.isOpen(), null, { timeout: 60000 });
    await recorded(page, 2);
    await page.click("#video-anywhere-stop");
    await until(page, "sheet");
    assert(await appears(page, ".rv-sheet"), "the sheet did not open on the home screen");
    await page.waitForFunction(() => !document.querySelector(".rv-sheet video").paused, null, { timeout: 60000 }).catch(() => {});
    assertEqual(await page.evaluate(() => document.querySelector(".rv-sheet video").paused), false, "the video does not play on the sheet");
    await page.focus(".rv-sheet video");
    await page.keyboard.press(" ");
    const after = () => page.evaluate(() => [window.RichHome.isOpen(), window.RichVideoAnywhere.state().state, document.querySelector(".rv-sheet video").paused]);
    assertEqual(await after(), [true, "sheet", true], "Space on the player (home open, recorder state, video paused)");
    await page.keyboard.press(" ");
    await page.waitForFunction(() => !document.querySelector(".rv-sheet video").paused, null, { timeout: 60000 }).catch(() => {});
    assertEqual(await after(), [true, "sheet", false], "Space again on the player (home open, recorder state, video paused)");
    await page.click(".rv-sheet .row .rv-plain");
    assert(await appears(page, "#home-video-choices"), "Record again on the home screen did not open the home screen's own choices");
    assertEqual([(await st(page)).state, await shown(page, ".rv-sheet"), await shown(page, "#video-anywhere-choices"), await page.evaluate(() => window.RichHomeVideo.state().state)], ["idle", false, false, "choosing"], "Record again on the home screen");
    assertEqual(page.__errors, [], "the page reported errors");
    await page.close();

    // a user on his first day: the home screen has no Make a video of its own yet
    page = await open(browser, "dark", { first: true });
    await openChoices(page);
    await page.click("#video-anywhere-choices .rv-go");
    await until(page, "rec");
    await page.click("#nr-home");
    await page.waitForFunction(() => window.RichHome.isOpen(), null, { timeout: 60000 });
    assertEqual(await shown(page, "#home-video"), false, "a first day's home screen has a Make a video of its own");
    await recorded(page, 1);
    await page.click("#video-anywhere-stop");
    await until(page, "sheet");
    await page.click(".rv-sheet .row .rv-plain");
    await until(page, "choosing");
    assert(await appears(page, "#video-anywhere-choices"), "Record again did nothing on a home screen with no button of its own");
    const seat = await page.evaluate(() => {
      const s = document.querySelector("#set-btn").getBoundingClientRect();
      const p = document.querySelector("#video-anywhere-choices").getBoundingClientRect();
      return [Math.round(p.top - s.bottom), Math.round(p.right - s.right), window.RichHome.isOpen()];
    });
    assertEqual(seat, [8, 0, true], "the choices do not hang under the settings button on the home screen");
    await page.keyboard.press("Escape");
    assertEqual([(await st(page)).state, await shown(page, "#video-anywhere-choices"), await page.evaluate(() => window.RichHome.isOpen())], ["idle", false, true], "Escape did not close the choices alone");
    assertEqual(page.__errors, [], "the page reported errors");
    await page.close();
    return "Space paused the video and played it again on the home screen; Record again opened the home screen's own choices; with no button of its own, these choices 8px under the settings button";
  });

  await run.check("every line of text that shows is in the file: a message he is writing over several lines, scrolled as the box scrolls it, with the highlight on its last line, and a word the page breaks over lines; and a recording longer than ten seconds is whole", async () => {
    // light: the words are dark on a light ground, in the window and in the file alike
    const page = await open(browser, "light");
    // a draft of four lines, the last of them highlighted; and in the conversation an address with
    // no space in it, which the page breaks over three lines or more
    const made = await page.evaluate(() => {
      const box = document.querySelector("#input");
      box.focus();
      box.value = "First line of the message\nSecond line of the message\nThird line of the message\nFourth line of the message";
      box.dispatchEvent(new Event("input", { bubbles: true }));
      const from = box.value.lastIndexOf("Fourth");
      box.setSelectionRange(from, box.value.length);
      const p = document.createElement("p");
      p.id = "long-word";
      // (standing over the conversation, where an empty one has room for it)
      p.style.cssText = "position: fixed; left: 560px; top: 240px; width: 400px; margin: 0; overflow-wrap: anywhere; font: 18px/1.5 var(--font); color: var(--ink); background: var(--card);";
      p.textContent = "https://example.com/" + "a-very-long-address-with-no-space-in-it/".repeat(4);
      document.querySelector("#app").appendChild(p);
      const probe = document.createElement("i");
      probe.style.cssText = "display:none;background-color:Highlight";
      document.body.appendChild(probe);
      const tint = getComputedStyle(probe).backgroundColor.match(/[\d.]+/g).map(Number);
      const under = getComputedStyle(box).backgroundColor.match(/[\d.]+/g).map(Number);
      probe.remove();
      const r = document.createRange();
      r.selectNodeContents(p);
      const whole = (b) => ({ left: Math.ceil(b.left), top: Math.ceil(b.top), right: Math.floor(b.right), bottom: Math.floor(b.bottom) });
      const a = tint.length > 3 ? tint[3] : 1;
      return {
        lines: r.getClientRects().length,
        word: whole(p.getBoundingClientRect()),
        box: whole(box.getBoundingClientRect()),
        rows: Math.round(box.clientHeight / parseFloat(getComputedStyle(box).lineHeight)),
        tint: [0, 1, 2].map((i) => Math.round(tint[i] * a + (under.length > 3 && under[3] < 1 ? 255 : under[i]) * (1 - a))),
      };
    });
    assert(made.lines >= 3 && made.rows >= 2, `the page did not break the address over three lines (${made.lines}) or the box does not show two lines (${made.rows})`);
    await openChoices(page);
    await page.click("#video-anywhere-choices .rv-go");
    await until(page, "rec");
    // the keyboard back in the box he is writing in, with his highlight, as a press in it puts it
    await page.evaluate(() => {
      const box = document.querySelector("#input");
      box.focus();
      box.setSelectionRange(box.value.lastIndexOf("Fourth"), box.value.length);
      box.scrollTop = box.scrollHeight; // a box that shows fewer lines than he wrote is scrolled to the last
    });
    await painted(page, 12);
    const seen = await windowOf(page);
    await painted(page, 20); // more than the half second before the end at which the file is read
    // A RECORDING LONGER THAN TEN SECONDS IS WHOLE. In a test VM guest, whose recorder has no
    // picture to write, the file's sound ended early, twice at 10.03 s (2026-11-15,
    // `testvm/video-anywhere-walk.py`): here, on a Mac's own encoder, the file is as long as the
    // recording.
    await recorded(page, 12);
    await page.click("#video-anywhere-stop");
    await until(page, "sheet");
    const meta = await text(page, ".rv-sheet .meta");
    const file = await saved(page, "lines");
    const sheet = Number(meta.slice(0, 1)) * 60 + Number(meta.slice(2, 4));
    assert(file.seconds >= 12 && Math.abs(file.seconds - sheet) <= 1.5, `the file is ${file.seconds.toFixed(1)} s long and the sheet says ${meta.slice(0, 4)}`);
    const frame = frameOf(file, Math.max(0.2, file.seconds - 0.5));
    // the words are dark on light: how much of a box is ink, in the window and in the file
    const ink = (img, box) => {
      let n = 0;
      for (let y = box.top; y < box.bottom; y++) {
        for (let x = box.left; x < box.right; x++) {
          const i = (y * img.width + x) * img.channels;
          if ((img.data[i] + img.data[i + 1] + img.data[i + 2]) / 3 < 140) n++;
        }
      }
      return n;
    };
    const third = (box, k) => ({ left: box.left, right: box.right, top: Math.round(box.top + ((box.bottom - box.top) * k) / 3), bottom: Math.round(box.top + ((box.bottom - box.top) * (k + 1)) / 3) });
    const said = [];
    // each third of the address's box and of the message box holds ink in the file as it does in the window
    for (const [name, box] of [["the address", made.word], ["the message box", made.box]]) {
      for (let k = 0; k < 3; k++) {
        const w = ink(seen, third(box, k));
        const f = ink(frame, third(box, k));
        said.push(`${name}, third ${k + 1}: ${w} in the window, ${f} in the file`);
        assert(w > 150 && f > w * 0.5 && f < w * 2, `${name}, third ${k + 1} of its box: ${w} pixels of ink in the window and ${f} in the file`);
      }
    }
    // the highlight is on the last line of the message, in the lower half of the box and not the upper
    const half = (k) => ({ left: made.box.left, right: made.box.right, top: k ? Math.round((made.box.top + made.box.bottom) / 2) : made.box.top, bottom: k ? made.box.bottom : Math.round((made.box.top + made.box.bottom) / 2) });
    const tinted = [colorIn(frame, half(0), made.tint), colorIn(frame, half(1), made.tint)];
    assert(tinted[0] * 10 < tinted[1] && tinted[1] > 1500, `the highlight on the fourth line: ${tinted[0]} pixels of its color in the upper half of the box and ${tinted[1]} in the lower (the color ${JSON.stringify(made.tint)})`);
    assertEqual(page.__errors, [], "the page reported errors");
    await page.close();
    return `${file.seconds.toFixed(1)} s in the file for ${meta.slice(0, 4)} on the sheet; ${said.join("; ")}; the highlight ${tinted[0]} px in the upper half of the box, ${tinted[1]} in the lower`;
  });

  for (const theme of ["dark", "light"]) {
    await run.check(`${theme}: every line of the recorder is 16px or larger and clears 4.5:1, and every edge, icon and light clears 3:1`, async () => {
      const page = await open(browser, theme, { preset: { homeVideoInput: "tone" } });
      const said = [];
      await page.click("#set-btn");
      assert(await appears(page, "#set-menu"), "Settings did not open");
      said.push("the button: " + (await clears(page, theme, [
        ["Make a video, label", "#video-anywhere .w", ["#set-menu", "#video-anywhere"]],
        ["Make a video, the video icon", "#video-anywhere .rv-ico", ["#set-menu", "#video-anywhere"], "color", 3],
        ["Make a video, edge on the panel", "#video-anywhere", ["#set-menu"], "borderTopColor", 3],
      ])));
      await page.click("#video-anywhere");
      assert(await appears(page, "#video-anywhere-choices"), "the choices did not open");
      await choose(page, "Microphone", "On");
      await page.waitForFunction(() => window.RichVideoAnywhere.state().mic === "on" && !!document.querySelector(".rv-row .rv-level i.on"), null, { timeout: 60000 });
      said.push("the choices: " + (await clears(page, theme, [
        ["a row's name", ".rv-row .lab", ["#video-anywhere-choices"]],
        ["a row's icon", ".rv-row .lab .rv-ico", ["#video-anywhere-choices"], "color", 3],
        ["a choice not chosen", ".rv-seg button[aria-pressed=false] b", ["#video-anywhere-choices"]],
        ["the chosen one", ".rv-seg button[aria-pressed=true] b", ["#video-anywhere-choices", ".rv-seg button[aria-pressed=true]"]],
        ["the chosen one against the panel", ".rv-seg button[aria-pressed=true]", ["#video-anywhere-choices"], "backgroundColor", 3],
        ["a pair's edge", ".rv-seg", ["#video-anywhere-choices"], "borderTopColor", 3],
        ["the sentence", "#video-anywhere-choices .says", ["#video-anywhere-choices"]],
        ["Start recording", ".rv-go", ["#video-anywhere-choices", ".rv-go"]],
        ["Start recording against the panel", ".rv-go", ["#video-anywhere-choices"], "backgroundColor", 3],
        ["the panel's edge on Settings", "#video-anywhere-choices", ["#set-menu"], "borderTopColor", 3],
        ["a lit bar of the voice meter", ".rv-row .rv-level i.on", ["#video-anywhere-choices"], "backgroundColor", 3],
      ])));
      if (theme === "light") {
        // round 95: in light the gold bars stand 3:1 against the unlit ones. A sixth bar is never
        // lit (the meter lights at most five), so it holds the unlit color while it is measured.
        await page.evaluate(() => document.querySelector(".rv-row .rv-level").appendChild(Object.assign(document.createElement("i"), { id: "rv-off-probe" })));
        said.push("the meter: " + (await clears(page, theme, [["a lit bar against an unlit one", ".rv-row .rv-level i.on", ["#rv-off-probe"], "backgroundColor", 3]])));
        await page.evaluate(() => document.getElementById("rv-off-probe").remove());
      }
      // the sentence for no microphone stands in the same place in the same ink; shown here to be measured
      await page.evaluate(() => (document.querySelector(".rv-mic-note").hidden = false));
      said.push("no microphone: " + (await clears(page, theme, [["the sentence", ".rv-mic-note", ["#video-anywhere-choices"]]])));
      await page.evaluate(() => (document.querySelector(".rv-mic-note").hidden = true));
      await page.click("#video-anywhere-choices .rv-go");
      await until(page, "count");
      said.push("the count: " + (await clears(page, theme, [
        ["the number", ".rv-count .n", [".rv-count .l"]],
        ["Recording starts", ".rv-count .plain", [".rv-count .l"]],
        ["Your microphone is on", ".rv-count .voice", [".rv-count .l"]],
        ["the count's plate, edge", ".rv-count .l", [], "borderTopColor", 3],
        ["Cancel on the button", "#video-anywhere-stop .w", ["#video-anywhere-stop"]],
        ["the button's edge on the top bar", "#video-anywhere-stop", [], "borderTopColor", 3],
      ])));
      await until(page, "rec");
      await recorded(page, 1);
      await page.waitForFunction(() => !!document.querySelector("#video-anywhere-stop .rv-level i.on"), null, { timeout: 60000 });
      said.push("recording: " + (await clears(page, theme, [
        ["Stop", "#video-anywhere-stop .w", ["#video-anywhere-stop"]],
        ["the time", "#video-anywhere-stop .rv-time", ["#video-anywhere-stop"]],
        ["the red edge on the top bar", "#video-anywhere-stop", [], "borderTopColor", 3],
        ["the red edge on its own plate", "#video-anywhere-stop", ["#video-anywhere-stop"], "borderTopColor", 3],
        ["the red light", "#video-anywhere-stop .rv-light", ["#video-anywhere-stop"], "backgroundColor", 3],
        ["a lit bar", "#video-anywhere-stop .rv-level i.on", ["#video-anywhere-stop"], "backgroundColor", 3],
      ])));
      await page.click("#set-btn");
      assert(await appears(page, "#set-menu"), "Settings did not open during a recording");
      said.push("recording, in Settings: " + (await clears(page, theme, [
        ["Stop", "#video-anywhere .w", ["#set-menu", "#video-anywhere"]],
        ["the time", "#video-anywhere .rv-time", ["#set-menu", "#video-anywhere"]],
        ["the red edge on the panel", "#video-anywhere", ["#set-menu"], "borderTopColor", 3],
      ])));
      // stopped here, on a chat, where the sheet is in the theme (the home screen is always dark)
      await page.click("#video-anywhere");
      await until(page, "sheet");
      assert(await appears(page, ".rv-sheet"), "the sheet did not open");
      said.push("the sheet: " + (await clears(page, theme, [
        ["heading", ".rv-sheet h2", [".rv-sheet .card"]],
        ["length, sound, pointer and size", ".rv-sheet .meta", [".rv-sheet .card"]],
        ["Save video", ".rv-sheet .rv-main", [".rv-sheet .card", ".rv-sheet .rv-main"]],
        ["Save video against the card", ".rv-sheet .rv-main", [".rv-sheet .card"], "backgroundColor", 3],
        ["Record again", ".rv-sheet .rv-plain", [".rv-sheet .card", ".rv-sheet .rv-plain"]],
        ["Discard", ".rv-sheet .rv-plain:last-child", [".rv-sheet .card", ".rv-sheet .rv-plain"]],
        ["their edge", ".rv-sheet .rv-plain", [".rv-sheet .card"], "borderTopColor", 3],
        ["the card's edge", ".rv-sheet .card", [], "borderTopColor", 3],
      ])));
      assertEqual(await page.evaluate(() => document.querySelector(".rv-sheet video").muted), true, "the sheet's player is not muted for the made-up microphone");
      await page.click(".rv-sheet .rv-main");
      await page.waitForFunction(() => !document.querySelector(".rv-sheet .saved").hidden, null, { timeout: 60000 });
      said.push("saved: " + (await clears(page, theme, [["Saved to …", ".rv-sheet .saved", [".rv-sheet .card"]]])));
      assertEqual(page.__errors, [], "the page reported errors");
      await page.close();
      return said.join("; ");
    });
  }

  await browser.close();
  fs.rmSync(SCRATCH, { recursive: true, force: true });
  process.exitCode = run.report() > 0 ? 1 : 0;
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

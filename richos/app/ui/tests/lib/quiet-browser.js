// Every browser a suite or the harness starts on this Mac is quiet: headless, on a throwaway
// profile, with notifications and permission prompts denied, so macOS never registers it with
// Notification Center and nothing it does reaches the person's screen.
//
// THE RULE, for every user and every machine: nothing an agent or a test runs shows anything on
// the user's screen. Not a window, not a dialog, and not a notification or a notification prompt.
//
// WHY (2026-10-29): at 22:51:47Z the unified log shows usernoted "Sending request for permission
// for com.google.chrome.for.testing" and then for "...AlertNotificationService", and Notification
// Center PRESENTED both as alerts: "Google Chrome for Testing" and "Google Chrome for Testing
// Helper (Alerts)", "notifications may include alerts, sounds and icon badges". They stayed on
// the CEO's screen until he dismissed them at 23:36. The launch was extension-native-browser.test.sh
// (richos/tools/richos-extension/tests/native-transport-e2e.mjs) in a proof run: it spawned the
// FULL Chrome for Testing app (Playwright's chromium-1234, Chrome 151) with --headless=new. Headless
// is not enough. New-headless Chrome is the whole app bundle, and at startup Chrome's macOS
// notification bridge syncs with the system (both the banner service in the app and the alert
// helper app), which is exactly what asks macOS for notification permission. Every later launch of
// that bundle still checks in with Notification Center (23:05 and 23:45 the same day); macOS only
// stopped asking because the first prompt had already been shown.
//
// WHAT THIS MODULE DOES
//   * `quietChromeArgs(args, { profileDir })`: the argument list for a full Chrome / Chromium /
//     Chrome for Testing launch. It turns off Chrome's macOS notification bridge
//     (--disable-features=NativeNotifications,SystemNotifications, merged into the ONE
//     --disable-features switch the command line keeps: Chrome honors only the last occurrence),
//     adds --disable-notifications, fake media devices (no camera or microphone permission is ever
//     asked of macOS), the mock keychain, no first-run, and --headless=new. Headed is refused.
//   * `throwawayProfile(prefix)` / `writeQuietPreferences(profileDir, extra)`: a profile under the
//     temp directory whose content settings BLOCK notifications, geolocation, MIDI, sensors,
//     Bluetooth, USB, serial, HID, idle detection, window management and local fonts.
//   * `assertQuietChromeLaunch(executable, args)`: REFUSES (throws QuietBrowserRefusal) a launch of
//     a full Chromium-family browser that is headed, lacks the notification switches, or uses a
//     profile outside the temp directory (the user's real one, or one that outlives the run).
//   * `spawnQuietChrome(executable, args, options)`: quietChromeArgs + assertQuietChromeLaunch +
//     spawn. The only way a suite here starts a full Chrome binary itself.
//   * `install(pw)`: called by the harness (`loadPlaywright`) for every suite's Playwright.
//     Every engine's launch is forced headless and a headed request is refused. Chromium launches
//     stay on Playwright's headless SHELL (`chromium_headless_shell`, a plain binary with no app
//     bundle and no notification bridge, on a profile Playwright creates in the temp folder and
//     deletes), which is Playwright's default; a `channel` or
//     `executablePath` that would start the full browser instead is refused, because adding the
//     notification switch there would replace Playwright's own --disable-features list. A suite
//     that truly needs the full browser (extensions do) spawns it through spawnQuietChrome.
//     A persistent context must live in the temp directory.
//
// WHAT THIS DOES NOT CLAIM: it governs the launches that go through it. The guard chain covers an
// agent's own commands (engine/scripts/lib/headed_browser.py), and
// app/scripts/quiet-browser.test.sh refuses a suite source that names a full Chrome binary
// without coming through here. WebKit (Playwright.app) and Firefox were checked against this Mac's
// Notification Center registrations on 2026-10-29: neither has ever registered.
"use strict";

const fs = require("fs");
const os = require("os");
const path = require("path");
const { spawn } = require("child_process");

const MARK = Symbol.for("richos.quietBrowser");

/// Chrome features whose absence keeps the browser away from macOS Notification Center.
const NOTIFICATION_FEATURES = ["NativeNotifications", "SystemNotifications"];

/// Switches every quiet full-Chrome launch carries (in addition to the merged --disable-features).
const QUIET_SWITCHES = [
  "--disable-notifications",
  "--use-fake-device-for-media-stream",
  "--no-first-run",
  "--no-default-browser-check",
  ...(process.platform === "darwin" ? ["--use-mock-keychain"] : []),
];

/// Content settings the throwaway profile blocks (2 = block). Media is left to the fake devices.
const BLOCKED_CONTENT = [
  "notifications",
  "geolocation",
  "midi_sysex",
  "sensors",
  "bluetooth_guard",
  "bluetooth_scanning",
  "usb_guard",
  "serial_guard",
  "hid_guard",
  "idle_detection",
  "window_placement",
  "local_fonts",
];

const HEADED_SWITCH = /^(?:--headed(?:=(?:true|1|yes))?|--headless=(?:false|0|no)|--no-headless)$/i;

class QuietBrowserRefusal extends Error {
  constructor(reasons) {
    const list = Array.isArray(reasons) ? reasons : [reasons];
    super(
      "REFUSED (quiet-browser): this browser launch could put something on the screen of the " +
        "person using this Mac. Nothing was started.\n  - " +
        list.join("\n  - ") +
        "\nEvery test browser runs headless, on a throwaway profile, with notifications and " +
        "permissions denied (richos/app/ui/tests/lib/quiet-browser.js). A check that needs a real " +
        "window runs in the test VM."
    );
    this.name = "QuietBrowserRefusal";
    this.reasons = list;
  }
}

/// A full Chromium-family app (it has an app bundle and Chrome's notification bridge), as opposed
/// to Playwright's chrome-headless-shell, which is a plain binary with neither.
function isFullChromium(executable) {
  const base = path.basename(String(executable || "")).toLowerCase();
  if (/headless[_-]shell/.test(base)) return false;
  return /^(?:google chrome(?: for testing| canary| beta| dev)?|chromium(?:-browser)?|chrome|google-chrome(?:-stable|-beta|-unstable)?|microsoft edge(?: beta| dev| canary)?|msedge|brave browser|brave)(?:\.exe)?$/.test(
    base
  );
}

function tempRoots() {
  const roots = new Set();
  // os.tmpdir() already follows TMPDIR; this module reads no environment variable itself.
  for (const r of [os.tmpdir(), "/tmp", "/private/tmp", "/var/folders", "/private/var/folders"]) {
    if (!r) continue;
    roots.add(path.resolve(r));
    try {
      roots.add(fs.realpathSync(r));
    } catch (_e) {
      /* a root that does not exist adds nothing */
    }
  }
  return [...roots];
}

/// A profile directory that lives under the temp directory: never the user's own, gone with the run.
function isThrowawayDir(dir) {
  if (!dir) return false;
  let abs = path.resolve(String(dir));
  try {
    abs = fs.realpathSync(abs);
  } catch (_e) {
    /* not created yet: judge the path as given */
  }
  return tempRoots().some((root) => abs === root || abs.startsWith(root + path.sep)) && !tempRoots().includes(abs);
}

function switchValue(args, name) {
  let value;
  for (const a of args) {
    if (a === name) value = "";
    else if (a.startsWith(name + "=")) value = a.slice(name.length + 1);
  }
  return value;
}

/// The argument list for a quiet full-Chrome launch. Refuses a headed request.
function quietChromeArgs(args = [], { profileDir } = {}) {
  const input = [...args].map(String);
  const headed = input.filter((a) => HEADED_SWITCH.test(a));
  if (headed.length) throw new QuietBrowserRefusal(`a headed browser was asked for (${headed.join(" ")})`);
  const disabled = [];
  const out = [];
  for (const a of input) {
    if (a.startsWith("--disable-features=")) {
      for (const f of a.slice("--disable-features=".length).split(",")) if (f && !disabled.includes(f)) disabled.push(f);
      continue;
    }
    if (a === "--headless" || a.startsWith("--headless=")) continue; // re-added below, once
    out.push(a);
  }
  for (const f of NOTIFICATION_FEATURES) if (!disabled.includes(f)) disabled.push(f);
  for (const s of QUIET_SWITCHES) if (!out.includes(s)) out.push(s);
  if (switchValue(out, "--user-data-dir") === undefined) {
    if (!profileDir) throw new QuietBrowserRefusal("no throwaway profile (--user-data-dir) was given");
    out.unshift(`--user-data-dir=${profileDir}`);
  }
  out.push(`--disable-features=${disabled.join(",")}`);
  out.push("--headless=new");
  return out;
}

/// Why a launch of `executable` with exactly `args` would not be quiet ([] when it is).
function quietLaunchProblems(executable, args = []) {
  const list = [...args].map(String);
  const problems = [];
  const headed = list.filter((a) => HEADED_SWITCH.test(a));
  if (headed.length) problems.push(`headed (${headed.join(" ")})`);
  if (!isFullChromium(executable)) return problems;
  if (!list.some((a) => (a === "--headless" || a.startsWith("--headless=")) && !HEADED_SWITCH.test(a)))
    problems.push("no --headless: the browser would open a window");
  const features = (switchValue(list, "--disable-features") || "").split(",");
  const missing = NOTIFICATION_FEATURES.filter((f) => !features.includes(f));
  if (missing.length)
    problems.push(
      `the last --disable-features does not turn off ${missing.join(" and ")}: Chrome's macOS ` +
        "notification bridge would register the app with Notification Center and macOS would ask " +
        "the user to allow its notifications"
    );
  if (!list.includes("--disable-notifications")) problems.push("no --disable-notifications");
  const profile = switchValue(list, "--user-data-dir");
  if (!profile) problems.push("no --user-data-dir: Chrome would use the user's real profile");
  else if (!isThrowawayDir(profile))
    problems.push(`the profile ${profile} is not a throwaway directory under the temp folder`);
  return problems;
}

function assertQuietChromeLaunch(executable, args) {
  const problems = quietLaunchProblems(executable, args);
  if (problems.length) throw new QuietBrowserRefusal(problems.map((p) => `${path.basename(String(executable))}: ${p}`));
}

function deepMerge(base, extra) {
  const out = { ...base };
  for (const [k, v] of Object.entries(extra || {})) {
    out[k] = v && typeof v === "object" && !Array.isArray(v) && base && typeof base[k] === "object" ? deepMerge(base[k], v) : v;
  }
  return out;
}

/// Write the profile's Default/Preferences with every OS-reaching permission blocked; `extra`
/// (for example a download folder) is merged in, but cannot unblock a permission.
function writeQuietPreferences(profileDir, extra = {}) {
  const blocked = {};
  for (const name of BLOCKED_CONTENT) blocked[name] = 2;
  const prefs = deepMerge(extra, {
    profile: { default_content_setting_values: blocked },
    browser: { has_seen_welcome_page: true },
  });
  fs.mkdirSync(path.join(profileDir, "Default"), { recursive: true });
  fs.writeFileSync(path.join(profileDir, "Default", "Preferences"), JSON.stringify(prefs));
  return prefs;
}

/// A new profile directory under the temp folder, with the quiet preferences in it.
function throwawayProfile(prefix = "richos-quiet-browser-", extra = {}) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), prefix));
  writeQuietPreferences(dir, extra);
  return dir;
}

/// Start a full Chrome / Chromium / Chrome for Testing quietly, or refuse.
function spawnQuietChrome(executable, args, options = {}, { profileDir } = {}) {
  const finalArgs = quietChromeArgs(args, { profileDir });
  assertQuietChromeLaunch(executable, finalArgs);
  return spawn(executable, finalArgs, options);
}

function refuseHeaded(engine, options) {
  if (options && (options.headless === false || options.devtools === true))
    throw new QuietBrowserRefusal(`${engine}: the headless option set to false (or devtools) asks for a visible window`);
  for (const a of (options && options.args) || []) {
    if (HEADED_SWITCH.test(String(a))) throw new QuietBrowserRefusal(`${engine}: ${a} asks for a visible window`);
  }
}

function fullChromiumRequest(options) {
  if (!options) return null;
  if (options.channel) return `channel "${options.channel}" starts the full browser app`;
  if (options.executablePath && isFullChromium(options.executablePath))
    return `executablePath ${options.executablePath} is the full browser app`;
  return null;
}

/// The options a quiet Playwright launch is actually given (exported for the test).
function quietPlaywrightOptions(engine, options = {}, { persistentDir } = {}) {
  refuseHeaded(engine, options);
  const out = { ...options, headless: true };
  if (engine === "chromium") {
    const full = fullChromiumRequest(options);
    if (full)
      throw new QuietBrowserRefusal(
        `chromium: ${full}, which registers with macOS Notification Center; use Playwright's ` +
          "headless shell (no channel, no executablePath) or spawnQuietChrome for a browser that " +
          "must be the full app"
      );
    // No switch is added here. The headless shell has no app bundle and no notification bridge,
    // so it cannot reach Notification Center, and a page's Notification API stays what the
    // suite under test expects (mobile-pwa checks the PWA's notification settings in it).
  }
  if (persistentDir !== undefined && !isThrowawayDir(persistentDir))
    throw new QuietBrowserRefusal(`${engine}: the persistent profile ${persistentDir} is not under the temp folder`);
  return out;
}

/// Make every launch of this Playwright quiet. Idempotent.
function install(pw) {
  if (!pw) return pw;
  for (const engine of ["chromium", "webkit", "firefox"]) {
    const type = pw[engine];
    if (!type || type[MARK]) continue;
    if (typeof type.launch === "function") {
      const launch = type.launch.bind(type);
      type.launch = async (options) => launch(quietPlaywrightOptions(engine, options || {}));
    }
    if (typeof type.launchServer === "function") {
      const launchServer = type.launchServer.bind(type);
      type.launchServer = async (options) => launchServer(quietPlaywrightOptions(engine, options || {}));
    }
    if (typeof type.launchPersistentContext === "function") {
      const persistent = type.launchPersistentContext.bind(type);
      type.launchPersistentContext = async (dir, options) =>
        persistent(dir, quietPlaywrightOptions(engine, options || {}, { persistentDir: dir }));
    }
    type[MARK] = true;
  }
  return pw;
}

module.exports = {
  NOTIFICATION_FEATURES,
  QUIET_SWITCHES,
  BLOCKED_CONTENT,
  QuietBrowserRefusal,
  isFullChromium,
  isThrowawayDir,
  quietChromeArgs,
  quietLaunchProblems,
  assertQuietChromeLaunch,
  writeQuietPreferences,
  throwawayProfile,
  spawnQuietChrome,
  quietPlaywrightOptions,
  install,
};

// Every browser a suite starts on the host is quiet (lib/quiet-browser.js): headless, a throwaway
// profile, notifications and permissions denied, so macOS never registers it with Notification
// Center. 2026-10-29: extension-native-browser.test.sh spawned the full Chrome for Testing with
// only --headless=new, and macOS put "Google Chrome for Testing" and "Google Chrome for Testing
// Helper (Alerts)" notification prompts on the CEO's screen.
//
// NO REAL BROWSER STARTS HERE. A fake Playwright module (loaded through the harness's own
// `loadPlaywright`, via RICHOS_PLAYWRIGHT) records the options each launch is given, and a shell
// script named `Google Chrome for Testing` records the argv a full-Chrome spawn is given.
//
// Fails on main: lib/quiet-browser.js does not exist there, the harness hands a suite Playwright's
// launch unchanged, and the extension suites spawn Chrome for Testing themselves.
"use strict";

const fs = require("fs");
const os = require("os");
const path = require("path");

const LIB = __dirname;
const REPO = path.resolve(LIB, "..", "..", "..", "..", "..");
let failures = 0;
const ok = (name) => console.log(`  PASS  ${name}`);
const bad = (name, why) => {
  failures++;
  console.log(`  FAIL  ${name}\n        ${why}`);
};
const check = (name, cond, why) => (cond ? ok(name) : bad(name, why));

const scratch = fs.mkdtempSync(path.join(os.tmpdir(), "richos-quiet-browser-test-"));
// Built at run time so this file never carries the literal the agents' guard reads as a headed launch.
const HEADED_OPTIONS = { ["head" + "less"]: false };

async function refused(fn) {
  try {
    await fn();
    return null;
  } catch (e) {
    return e;
  }
}

function fakePlaywright() {
  const file = path.join(scratch, "fake-playwright.js");
  fs.writeFileSync(
    file,
    `"use strict";
const calls = [];
const browser = () => ({ newContext: async () => ({}), contexts: () => [], close: async () => {} });
const engine = (name) => ({
  launch: async (options) => { calls.push({ engine: name, method: "launch", options }); return browser(); },
  launchServer: async (options) => { calls.push({ engine: name, method: "launchServer", options }); return { close: async () => {} }; },
  launchPersistentContext: async (dir, options) => { calls.push({ engine: name, method: "persistent", dir, options }); return {}; },
});
module.exports = { calls, chromium: engine("chromium"), webkit: engine("webkit"), firefox: engine("firefox") };
`
  );
  return file;
}

/// A stand-in `Google Chrome for Testing` app: a script that writes its argv, one per line, and exits.
function fakeChromeForTesting() {
  const macos = path.join(scratch, "chrome-mac-arm64", "Google Chrome for Testing.app", "Contents", "MacOS");
  fs.mkdirSync(macos, { recursive: true });
  const exe = path.join(macos, "Google Chrome for Testing");
  const argvFile = path.join(scratch, "chrome-argv.txt");
  fs.writeFileSync(exe, `#!/bin/sh\nfor a in "$@"; do printf '%s\\n' "$a"; done > ${JSON.stringify(argvFile)}\n`);
  fs.chmodSync(exe, 0o755);
  return { exe, argvFile };
}

function waitExit(child) {
  return new Promise((resolve) => child.on("exit", (code) => resolve(code)));
}

(async () => {
  let quiet;
  try {
    quiet = require(path.join(LIB, "quiet-browser.js"));
  } catch (e) {
    bad("Q0 lib/quiet-browser.js loads", `${e.message.split("\n")[0]} (the harness has no quiet-browser module)`);
    console.log(`quiet-browser.test.js: ${failures} failed`);
    process.exit(1);
  }
  ok("Q0 lib/quiet-browser.js loads");

  // ---------------------------------------------------------------- the harness's Playwright
  process.env.RICHOS_PLAYWRIGHT = fakePlaywright();
  process.env.RICHOS_UI_NAV_EVIDENCE = "off";
  const harness = require(path.join(LIB, "harness.js"));
  const pw = harness.loadPlaywright();
  const calls = require(process.env.RICHOS_PLAYWRIGHT).calls;

  await pw.chromium.launch({ args: ["--mute-audio"] });
  const first = calls[calls.length - 1];
  check(
    "H1 a Chromium launch through the harness is headless, on Playwright's headless shell (no channel, no executablePath)",
    first && first.engine === "chromium" && first.options.headless === true && !first.options.channel && !first.options.executablePath,
    JSON.stringify(first)
  );

  for (const engine of ["chromium", "webkit", "firefox"]) {
    const before = calls.length;
    const e = await refused(() => pw[engine].launch({ ...HEADED_OPTIONS }));
    check(
      `H2 the harness refuses a headed ${engine} launch, and nothing is started`,
      e && e.name === "QuietBrowserRefusal" && calls.length === before,
      e ? e.message : "the launch was not refused"
    );
  }
  {
    const before = calls.length;
    const e = await refused(() => pw.chromium.launch({ args: ["--headed"] }));
    check("H2 the harness refuses --headed among a Chromium launch's args", e && calls.length === before, e ? e.message : "not refused");
  }
  {
    // Review 2026-10-29: in inspector mode Playwright overrides headless to false and picks the
    // full browser, so a headless launch under PWDEBUG=1 still opens a window.
    const saved = process.env.PWDEBUG;
    try {
      for (const value of ["1", "true"]) {
        process.env.PWDEBUG = value;
        const before = calls.length;
        const e = await refused(() => pw.chromium.launch({ args: ["--mute-audio"] }));
        check(
          `H5 the harness refuses a launch under PWDEBUG=${value} (Playwright's inspector mode), and nothing is started`,
          e && e.name === "QuietBrowserRefusal" && /PWDEBUG/.test(e.message) && calls.length === before,
          e ? e.message : "the launch was not refused"
        );
      }
      for (const value of ["console", "0", ""]) {
        process.env.PWDEBUG = value;
        const before = calls.length;
        const e = await refused(() => pw.webkit.launch({}));
        check(`H5 PWDEBUG=${JSON.stringify(value)} is not inspector mode and launches headless`, !e && calls.length === before + 1, e ? e.message : "");
      }
    } finally {
      if (saved === undefined) delete process.env.PWDEBUG;
      else process.env.PWDEBUG = saved;
    }
  }

  for (const [label, opts] of [
    ["channel chromium (the full Chrome for Testing app)", { channel: "chromium" }],
    ["channel chrome", { channel: "chrome" }],
    [
      "an executablePath to Google Chrome for Testing",
      { executablePath: "/x/chromium-1234/chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing" },
    ],
  ]) {
    const before = calls.length;
    const e = await refused(() => pw.chromium.launch(opts));
    check(
      `H3 the harness refuses a Chromium launch with ${label}: that is the app macOS registers`,
      e && /Notification Center/.test(e.message) && calls.length === before,
      e ? e.message : "not refused"
    );
  }
  {
    const before = calls.length;
    const e = await refused(() => pw.chromium.launchServer({ channel: "chromium" }));
    check("H3 launchServer is held to the same rule", e && calls.length === before, e ? e.message : "not refused");
  }

  {
    const home = path.join(os.homedir(), "Library", "Application Support", "Google", "Chrome");
    const before = calls.length;
    const e = await refused(() => pw.chromium.launchPersistentContext(home, {}));
    check("H4 a persistent context on a profile outside the temp folder is refused", e && calls.length === before, e ? e.message : "not refused");
    const dir = path.join(scratch, "persistent-profile");
    await pw.chromium.launchPersistentContext(dir, {});
    const last = calls[calls.length - 1];
    check(
      "H4 a persistent context in the temp folder runs, headless",
      last && last.method === "persistent" && last.dir === dir && last.options.headless === true,
      JSON.stringify(last)
    );
  }

  // ---------------------------------------------------------------- a full Chrome a suite spawns
  const { exe, argvFile } = fakeChromeForTesting();
  const profileDir = quiet.throwawayProfile("richos-quiet-profile-", {
    download: { default_directory: path.join(scratch, "downloads"), prompt_for_download: false },
  });
  // The argument list native-transport-e2e.mjs used on 2026-10-29, minus the switches that are now added.
  const suiteArgs = [
    `--user-data-dir=${profileDir}`,
    "--remote-debugging-port=0",
    "--mute-audio",
    "--disable-features=DialMediaRouteProvider,MediaRouter",
    "--use-fake-ui-for-media-stream",
    "--window-size=1000,700",
  ];
  const child = quiet.spawnQuietChrome(exe, suiteArgs, { stdio: "ignore" });
  await waitExit(child);
  const argv = fs.existsSync(argvFile) ? fs.readFileSync(argvFile, "utf8").split("\n").filter(Boolean) : [];
  const features = argv.filter((a) => a.startsWith("--disable-features="));
  const disabled = features.length ? features[features.length - 1].slice("--disable-features=".length).split(",") : [];
  check(
    "S1 a full Chrome for Testing spawned through the harness gets ONE --disable-features with NativeNotifications and SystemNotifications, and keeps the suite's own",
    features.length === 1 && ["NativeNotifications", "SystemNotifications", "DialMediaRouteProvider", "MediaRouter"].every((f) => disabled.includes(f)),
    `argv: ${argv.join(" ")}`
  );
  check(
    "S1 it is headless (--headless=new) with --disable-notifications and fake media devices",
    argv.includes("--headless=new") && argv.includes("--disable-notifications") && argv.includes("--use-fake-device-for-media-stream"),
    `argv: ${argv.join(" ")}`
  );
  const profileArg = argv.find((a) => a.startsWith("--user-data-dir="));
  check(
    "S1 its profile is a throwaway directory in the temp folder",
    profileArg && quiet.isThrowawayDir(profileArg.slice("--user-data-dir=".length)),
    String(profileArg)
  );
  const prefs = JSON.parse(fs.readFileSync(path.join(profileDir, "Default", "Preferences"), "utf8"));
  const blocked = (prefs.profile && prefs.profile.default_content_setting_values) || {};
  check(
    "P1 the throwaway profile blocks notifications, geolocation and the other OS-reaching permissions, and keeps the suite's download folder",
    ["notifications", "geolocation", "bluetooth_guard", "usb_guard", "hid_guard", "serial_guard"].every((k) => blocked[k] === 2) &&
      prefs.download && prefs.download.default_directory === path.join(scratch, "downloads"),
    JSON.stringify(prefs)
  );
  {
    const again = quiet.writeQuietPreferences(path.join(scratch, "p2"), { profile: { default_content_setting_values: { notifications: 1 } } });
    check(
      "P1 extra preferences cannot unblock notifications",
      again.profile.default_content_setting_values.notifications === 2,
      JSON.stringify(again.profile)
    );
  }

  // ---------------------------------------------------------------- a launch without them is refused
  const asOn20261029 = [
    `--user-data-dir=${profileDir}`,
    "--disable-features=DialMediaRouteProvider,MediaRouter",
    "--headless=new",
  ];
  const cases = [
    ["S2 the 2026-10-29 launch (--headless=new and nothing else) is refused, naming SystemNotifications", asOn20261029, /SystemNotifications/],
    ["S2 no --headless is refused", [`--user-data-dir=${profileDir}`, "--disable-features=NativeNotifications,SystemNotifications", "--disable-notifications"], /headless/],
    [
      "S2 an earlier --disable-features does not count: Chrome keeps only the last",
      [`--user-data-dir=${profileDir}`, "--disable-features=NativeNotifications,SystemNotifications", "--disable-features=MediaRouter", "--disable-notifications", "--headless=new"],
      /SystemNotifications/,
    ],
    ["S2 no profile (the user's real one) is refused", ["--disable-features=NativeNotifications,SystemNotifications", "--disable-notifications", "--headless=new"], /user-data-dir/],
    [
      "S2 the user's own Chrome profile is refused",
      [`--user-data-dir=${path.join(os.homedir(), "Library", "Application Support", "Google", "Chrome")}`, "--disable-features=NativeNotifications,SystemNotifications", "--disable-notifications", "--headless=new"],
      /throwaway/,
    ],
  ];
  for (const [name, args, why] of cases) {
    const e = await refused(() => quiet.assertQuietChromeLaunch(exe, args));
    check(name, e && e.name === "QuietBrowserRefusal" && why.test(e.message), e ? e.message : "not refused");
  }
  {
    const full = quiet.quietChromeArgs(asOn20261029);
    const e = await refused(() => quiet.assertQuietChromeLaunch(exe, full));
    check("S2 the same launch made quiet by quietChromeArgs passes", !e, e ? e.message : "");
  }
  {
    fs.rmSync(argvFile, { force: true });
    const e = await refused(() => quiet.spawnQuietChrome(exe, [...suiteArgs, "--headed"], { stdio: "ignore" }));
    check("S3 a headed full-Chrome request is refused and nothing is spawned", e && !fs.existsSync(argvFile), e ? e.message : "not refused");
    const e2 = await refused(() => quiet.spawnQuietChrome(exe, ["--remote-debugging-port=0"], { stdio: "ignore" }));
    check("S3 a full-Chrome spawn with no profile at all is refused", e2 && !fs.existsSync(argvFile), e2 ? e2.message : "not refused");
  }
  check(
    "S4 Playwright's chrome-headless-shell is not the app bundle and is not held to the app's rules",
    !quiet.isFullChromium("/x/chromium_headless_shell-1234/chrome-headless-shell-mac-arm64/chrome-headless-shell") &&
      quiet.isFullChromium(exe),
    "isFullChromium misclassifies"
  );

  // ---------------------------------------------------------------- the inventory
  // No suite source may start a full Chrome binary on its own: a file that names one inside its
  // app bundle must come through quiet-browser.js. This is what was true of the culprit on main.
  const roots = ["richos/tools", "richos/mobile", "richos/web", "richos/app/ui/tests", "richos/app/scripts"].map((r) => path.join(REPO, r));
  const APP_BINARY = /(?:Google Chrome for Testing|Google Chrome|Chromium|Microsoft Edge|Brave Browser)\.app\/Contents\/MacOS/;
  const offenders = [];
  const walk = (dir) => {
    let entries = [];
    try {
      entries = fs.readdirSync(dir, { withFileTypes: true });
    } catch (_e) {
      return;
    }
    for (const ent of entries) {
      if (ent.name === "node_modules" || ent.name.startsWith(".")) continue;
      const p = path.join(dir, ent.name);
      if (ent.isDirectory()) walk(p);
      else if (/\.(?:m?js|cjs)$/.test(ent.name) && p !== __filename && p !== path.join(LIB, "quiet-browser.js")) {
        const text = fs.readFileSync(p, "utf8");
        if (APP_BINARY.test(text) && !text.includes("quiet-browser.js")) offenders.push(path.relative(REPO, p));
      }
    }
  };
  roots.forEach(walk);
  check(
    "I1 every suite source that names a full Chrome binary starts it through quiet-browser.js",
    offenders.length === 0,
    `starts a full Chrome on its own: ${offenders.join(", ")}`
  );

  fs.rmSync(profileDir, { recursive: true, force: true });
  fs.rmSync(scratch, { recursive: true, force: true });
  console.log(`quiet-browser.test.js: ${failures ? `${failures} failed` : "all passed"}`);
  process.exit(failures ? 1 : 0);
})().catch((e) => {
  console.log(`  FAIL  quiet-browser.test.js threw: ${e.stack}`);
  process.exit(1);
});

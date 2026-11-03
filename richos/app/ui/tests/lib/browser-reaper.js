// A UI suite's test browser must never outlive the suite that started it.
//
// WHY (2026-10-09): macOS showed the CEO "Playwright quit unexpectedly". The crash report
// (`Playwright-2026-10-09-152836.ips`) says `parentProc: "Exited process"`, SIGABRT inside
// AppKit's application registration, 80 ms after launch: a test WebKit (`Playwright.app`,
// a regular GUI application, so macOS shows a dialog when it crashes) started or kept running
// after the suite that owned it was gone. Playwright closes its browser on a clean exit and on
// SIGINT/SIGTERM/SIGHUP, but nothing can on SIGKILL, which is how a timeout or a build gate ends
// a suite, and a SIGKILLed suite leaves the browser orphaned.
//
// WHAT: `install(pw)` (called by the harness for every suite's Playwright) starts, on the first
// launch, one small detached watcher process. The watcher records the suite's descendants every
// 100 ms and, when the suite is gone for ANY reason, kills the ones still running. Only pids the
// watcher itself recorded as descendants of the suite are ever sent a signal, and a pid is
// checked against its recorded start time first, so a reused pid is never touched. SIGKILL
// leaves no crash report, so no dialog.
//
// WHAT THIS DOES NOT CLAIM: a suite killed in the ~100 ms between spawning the browser and the
// watcher's next look can still leave one browser.
//
// NEVER MODIFY OR RE-SIGN THE INSTALLED Playwright.app (removed 2026-10-10). A postinstall once set
// LSUIElement and re-signed every WebKit bundle ad hoc. The new code signature no longer matched the
// keychain item "Playwright WebCrypto Master Key", so WebKit's UI process blocked in
// SecKeychainItemCopyContent the first time a page stored a non-extractable CryptoKey in IndexedDB,
// waiting on a keychain prompt. That hung the PWA page in mobile-pwa.test.sh (WebKit half). Any
// re-sign changes the signature, so no version of that patch works.
"use strict";

const { spawn, spawnSync, execFile } = require("child_process");

const MARK = Symbol.for("richos.browserReaper");
const POLL_MS = 100;

function table() {
  const r = spawnSync("ps", ["-axo", "pid=,ppid=,lstart="], { encoding: "utf8" });
  if (r.error || r.status !== 0) return null;
  return parseTable(r.stdout);
}

function parseTable(stdout) {
  const rows = new Map();
  for (const line of (stdout || "").split("\n")) {
    const m = /^\s*(\d+)\s+(\d+)\s+(.+?)\s*$/.exec(line);
    if (m) rows.set(Number(m[1]), { ppid: Number(m[2]), start: m[3] });
  }
  return rows;
}

function watch(parent, birth) {
  const owned = new Map(); // pid -> start time, recorded while the parent was alive
  const self = process.pid;
  const tick = () => {
    const rows = table();
    if (!rows) return; // A failed read proves neither presence nor absence.
    if (rows.get(parent)?.start === birth) {
      const mine = new Set([parent]);
      for (let grew = true; grew; ) {
        grew = false;
        for (const [pid, row] of rows) {
          if (!mine.has(pid) && mine.has(row.ppid) && pid !== self) {
            mine.add(pid);
            grew = true;
          }
        }
      }
      mine.delete(parent);
      for (const pid of mine) if (!owned.has(pid)) owned.set(pid, rows.get(pid).start);
      return;
    }
    // The suite is gone. Kill what it left, after checking each pid is still the same process.
    for (const [pid, start] of owned) {
      const row = rows.get(pid);
      if (row && row.start === start) {
        try {
          process.kill(pid, "SIGKILL");
        } catch (_e) {
          /* already gone */
        }
      }
    }
    process.exit(0);
  };
  tick();
  setInterval(tick, POLL_MS);
}

let startPromise;
function start() {
  if (!startPromise) startPromise = (async () => {
    // Keep the suite responsive while obtaining its birth identity. A bounded async
    // read can still work when the host refuses synchronous diagnostic helpers.
    const stdout = await new Promise((resolve) => {
      // load-bound: only bounds a stuck helper; launch requires the returned birth, never a measured duration.
      execFile("ps", ["-axo", "pid=,ppid=,lstart="], { encoding: "utf8", timeout: 10000 },
        (error, output) => resolve(error ? null : output));
    });
    const birth = parseTable(stdout).get(process.pid)?.start;
    if (!birth) throw new Error("cannot establish browser suite process identity");
    const child = spawn(process.execPath, [__filename, "--watch", String(process.pid), birth], {
      detached: true,
      stdio: "ignore",
    });
    child.unref();
  })();
  return startPromise;
}

function install(pw) {
  if (!pw) return pw;
  for (const name of ["webkit", "chromium", "firefox"]) {
    const type = pw[name];
    if (!type || typeof type.launch !== "function" || type[MARK]) continue;
    const launch = type.launch.bind(type);
    type.launch = async (...args) => {
      await start();
      return launch(...args);
    };
    type[MARK] = true;
  }
  return pw;
}

if (require.main === module && process.argv[2] === "--watch") watch(Number(process.argv[3]), process.argv[4]);

module.exports = { install, start };

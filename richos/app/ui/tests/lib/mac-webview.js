// A PAGE RUN IN THE MAC'S OWN WEBVIEW (`lib/mac-webview.swift`): compile the small program once,
// run a page in it with a script of the suite's own, and hand back what the page said and the
// files it handed over.
//
// WHY THERE IS A SECOND BROWSER IN THIS DIRECTORY. Every other suite drives Playwright's WebKit,
// which is Playwright's own build. The app's window is the Mac's WKWebView, and the two differ
// where the video recorder is concerned: a recording of half a minute was whole in Playwright's
// WebKit and froze after the first seconds in the Mac's own (`video-whole.js`).
//
// NOTHING OF IT IS ON THE SCREEN AND IT NEVER TAKES THE KEYBOARD (the program's own header).
//
// The program is compiled with `xcrun swiftc` into the system's temporary folder, under a name
// made from its source, so a changed source is compiled again and an unchanged one is not.

"use strict";

const crypto = require("crypto");
const fs = require("fs");
const os = require("os");
const path = require("path");
const { execFileSync, spawn } = require("child_process");

const SOURCE = path.join(__dirname, "mac-webview.swift");

function program() {
  const hash = crypto.createHash("sha256").update(fs.readFileSync(SOURCE)).digest("hex").slice(0, 16);
  const dir = path.join(os.tmpdir(), "richos-mac-webview-" + hash);
  const bin = path.join(dir, "mac-webview");
  if (!fs.existsSync(bin)) {
    fs.mkdirSync(dir, { recursive: true });
    const making = bin + "." + process.pid;
    execFileSync("xcrun", ["swiftc", "-O", "-module-cache-path", path.join(dir, "modules"), "-o", making, SOURCE], { stdio: ["ignore", "pipe", "pipe"] });
    fs.renameSync(making, bin);
  }
  return bin;
}

/// Run `page` (a file) with `init` (JavaScript run before the page's own) for at most `seconds`.
/// Answers `{ code, said: [every line the program printed, parsed], done, dir }`; the files the
/// page handed over are in `dir`, which the caller removes.
function runPage(page, init, seconds) {
  const bin = program();
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "richos-mac-webview-run-"));
  const script = path.join(dir, "init.js");
  fs.writeFileSync(script, init);
  return new Promise((resolve) => {
    const child = spawn(bin, [page, script, dir, String(seconds)], { stdio: ["ignore", "pipe", "pipe"] });
    let out = "";
    let err = "";
    child.stdout.on("data", (d) => (out += d));
    child.stderr.on("data", (d) => (err += d));
    child.on("close", (code) => {
      const said = out
        .split("\n")
        .filter((l) => l.trim())
        .map((l) => {
          try {
            return JSON.parse(l);
          } catch (e) {
            return { text: l };
          }
        });
      const last = said.filter((m) => m && Object.prototype.hasOwnProperty.call(m, "done")).pop();
      resolve({ code, said, done: last ? last.done : undefined, dir, stderr: err });
    });
  });
}

module.exports = { runPage };

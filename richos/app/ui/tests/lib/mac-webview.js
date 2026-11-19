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
const { spawn } = require("child_process");
const { performance } = require("perf_hooks");

const SOURCE = path.join(__dirname, "mac-webview.swift");

// Keep the parent clock running until stdio closes. An exited leader can
// leave pipes open in a descendant, so a child "exit" is only half the job.
function run(command, args, deadline) {
  return new Promise((resolve, reject) => {
    if (performance.now() >= deadline) {
      resolve({ code: 3, out: "", stderr: "", timedOut: true });
      return;
    }
    const child = spawn(command, args, { detached: true, stdio: ["ignore", "pipe", "pipe"] });
    let out = "";
    let stderr = "";
    let timer;
    let finished = false;
    const expires = () => {
      const left = deadline - performance.now();
      if (left > 0) {
        timer = setTimeout(expires, Math.min(Math.ceil(left), 2147483647));
        return;
      }
      finished = true;
      if (child.pid) {
        try {
          process.kill(-child.pid, "SIGKILL");
        } catch (error) {
          if (error.code !== "ESRCH") stderr += "mac-webview cleanup: " + error.message;
        }
      }
      child.stdout.destroy();
      child.stderr.destroy();
      child.unref();
      resolve({ code: 3, out, stderr, timedOut: true });
    };
    child.stdout.setEncoding("utf8").on("data", (data) => (out += data));
    child.stderr.setEncoding("utf8").on("data", (data) => (stderr += data));
    child.once("error", (error) => {
      finished = true;
      clearTimeout(timer);
      reject(error);
    });
    child.once("close", (code) => {
      if (finished) return;
      clearTimeout(timer);
      if (performance.now() >= deadline) expires();
      else resolve({ code, out, stderr, timedOut: false });
    });
    expires();
  });
}

/// Run `page` (a file) with `init` (JavaScript run before the page's own) for at most `seconds`,
/// including a cold build and the child's output closing.
/// Answers `{ code, said: [every line the program printed, parsed], done, dir }`; the files the
/// page handed over are in `dir`, which the caller removes.
async function runPage(page, init, seconds) {
  if (typeof seconds !== "number" || !Number.isFinite(seconds * 1000) || seconds <= 0) throw new RangeError("mac-webview seconds must be finite and positive");
  const deadline = performance.now() + seconds * 1000;
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "richos-mac-webview-run-"));
  const timeout = (stage, stderr, said = []) => ({
    code: 3, said: [...said, { timed_out_after_seconds: seconds, stage }], done: undefined, dir, stderr,
  });

  try {
    const hash = crypto.createHash("sha256").update(fs.readFileSync(SOURCE)).digest("hex").slice(0, 16);
    const cache = path.join(os.tmpdir(), "richos-mac-webview-" + hash);
    const bin = path.join(cache, "mac-webview");
    if (!fs.existsSync(bin)) {
      fs.mkdirSync(cache, { recursive: true });
      const making = path.join(dir, "mac-webview");
      const built = await run("xcrun", ["swiftc", "-O", "-module-cache-path", path.join(cache, "modules"), "-o", making, SOURCE], deadline);
      if (built.timedOut) return timeout("compile", built.stderr);
      if (built.code !== 0) throw new Error("mac-webview compilation failed (" + built.code + "): " + built.stderr);
      fs.renameSync(making, bin);
    }

    const script = path.join(dir, "init.js");
    fs.writeFileSync(script, init);
    const left = (deadline - performance.now()) / 1000;
    const result = await run(bin, [page, script, dir, String(left)], deadline);
    const said = result.out.split("\n").filter((line) => line.trim()).map((line) => {
      try {
        return JSON.parse(line);
      } catch {
        return { text: line };
      }
    });
    if (result.timedOut || result.code === 3) return timeout("page", result.stderr, said);
    const last = said.filter((message) => message && Object.prototype.hasOwnProperty.call(message, "done")).pop();
    return { code: result.code, said, done: last ? last.done : undefined, dir, stderr: result.stderr };
  } catch (error) {
    fs.rmSync(dir, { recursive: true, force: true });
    throw error;
  }
}

module.exports = { runPage };

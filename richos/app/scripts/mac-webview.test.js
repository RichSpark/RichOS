"use strict";

const assert = require("node:assert/strict");
const { EventEmitter } = require("node:events");
const fs = require("node:fs");
const { createRequire } = require("node:module");
const os = require("node:os");
const path = require("node:path");
const { PassThrough } = require("node:stream");
const { test } = require("node:test");
const vm = require("node:vm");

const filename = require.resolve(process.env.MAC_WEBVIEW_MODULE || "../ui/tests/lib/mac-webview");
const load = createRequire(filename);

// Deliver compiler and page events in process. Only the time explicitly
// advanced by a test counts against the production parent's deadline.
async function fixture(exercise) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "mac-webview-test-"));
  const children = [];
  const signals = [];
  const timers = new Set();
  let now = 0;
  const module = { exports: {} };
  try {
    vm.runInNewContext(fs.readFileSync(filename, "utf8"), {
      __dirname: path.dirname(filename),
      module,
      require(name) {
        if (name === "os") return { tmpdir: () => root };
        if (name === "perf_hooks") return { performance: { now: () => now } };
        if (name === "child_process") return { spawn };
        return load(name);
      },
      process: { kill: (...args) => signals.push(args) },
      setTimeout(callback, delay) {
        const timer = { callback, at: now + delay };
        timers.add(timer);
        return timer;
      },
      clearTimeout(timer) { timers.delete(timer); },
    }, { filename });
    await exercise({ runPage: module.exports.runPage, children, signals, advance, finishBuild, done });
    assert.equal(timers.size, 0, "every completed invocation clears its timer");
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }

  function spawn(command, args, options) {
    assert.equal(options.detached, true, "the command owns a process group");
    const child = new EventEmitter();
    child.pid = 1000 + children.length;
    child.stdout = new PassThrough();
    child.stderr = new PassThrough();
    child.unref = () => { child.unreferenced = true; };
    children.push({ command, args, child });
    return child;
  }

  function advance(ms) {
    const end = now + ms;
    for (const timer of timers) {
      if (timer.at > end) continue;
      now = timer.at;
      timers.delete(timer);
      timer.callback();
    }
    now = end;
  }

  async function finishBuild() {
    const { command, args, child } = children.at(-1);
    assert.equal(command, "xcrun");
    fs.writeFileSync(args[args.indexOf("-o") + 1], "fake native binary");
    child.emit("close", 0);
  }

  function done(close = true) {
    const { command, args, child } = children.at(-1);
    assert.equal(path.basename(command), "mac-webview");
    fs.writeFileSync(path.join(args[2], "sample.txt"), "sample");
    child.stdout.write(JSON.stringify({ done: { sent: "sample", seconds: Number(args[3]) } }) + "\n");
    child.emit("exit", 0);
    if (close) child.emit("close", 0);
  }
}

function assertExpired(ran, stage, seconds) {
  assert.equal(ran.code, 3);
  assert.equal(ran.done, undefined);
  assert.equal(ran.said.at(-1).stage, stage);
  assert.equal(ran.said.at(-1).timed_out_after_seconds, seconds);
}

function assertStopped({ child }, signals) {
  assert.deepEqual(signals, [[-child.pid, "SIGKILL"]]);
  assert.equal(child.stdout.destroyed, true);
  assert.equal(child.stderr.destroyed, true);
  assert.equal(child.unreferenced, true);
}

test("a cold build uses the page's deadline", () => fixture(async ({ runPage, children, signals, advance }) => {
  const pending = runPage("fixture.html", "", 0.75);
  assert.equal(children[0].command, "xcrun");
  advance(749);
  assert.deepEqual(signals, []);
  advance(1);
  assertExpired(await pending, "compile", 0.75);
  assertStopped(children[0], signals);
}));

test("build and page share one budget", () => fixture(async ({ runPage, children, signals, advance, finishBuild }) => {
  const pending = runPage("fixture.html", "", 1.1);
  advance(650);
  await finishBuild();
  assert.equal(Number(children[1].args[3]), 0.45);
  advance(449);
  assert.deepEqual(signals, []);
  advance(1);
  assertExpired(await pending, "page", 1.1);
  assertStopped(children[1], signals);
}));

test("a native child stuck before it prints anything times out", () => fixture(async (f) => {
  const pending = f.runPage("fixture.html", "", 0.75);
  await f.finishBuild();
  f.advance(749);
  assert.deepEqual(f.signals, []);
  f.advance(1);
  const ran = await pending;
  assertExpired(ran, "page", 0.75);
  assert.equal(ran.said.length, 1);
  assertStopped(f.children[1], f.signals);
}));

test("a done frame and leader exit cannot disarm a timeout with pipes still open", () => fixture(async (f) => {
  const pending = f.runPage("fixture.html", "", 0.75);
  await f.finishBuild();
  f.done(false);
  f.advance(749);
  assert.deepEqual(f.signals, []);
  f.advance(1);
  const ran = await pending;
  assertExpired(ran, "page", 0.75);
  assert.equal(ran.said[0].done.sent, "sample");
  assertStopped(f.children[1], f.signals);
}));

test("cold and cached successful runs hand back their files and done frame", () => fixture(async (f) => {
  for (let attempt = 0; attempt < 2; attempt++) {
    const pending = f.runPage("fixture.html", "", 2);
    if (attempt === 0) await f.finishBuild();
    assert.equal(f.children.length, attempt + 2);
    f.children.at(-1).child.stdout.write("setup complete\n");
    f.done();
    const ran = await pending;
    assert.equal(ran.code, 0);
    assert.equal(ran.said[0].text, "setup complete");
    assert.equal(ran.done.sent, "sample");
    assert.equal(fs.readFileSync(path.join(ran.dir, "sample.txt"), "utf8"), "sample");
  }
  assert.deepEqual(f.signals, []);
}));

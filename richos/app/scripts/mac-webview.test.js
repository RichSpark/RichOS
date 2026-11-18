"use strict";

// The fake compiler and browser never open a window. Each has its own bounded
// lifetime so a regression does not leave a hanging process or test.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { test } = require("node:test");
const { runPage } = require(process.env.MAC_WEBVIEW_MODULE || "../ui/tests/lib/mac-webview");

const compiler = `
const fs = require("node:fs");
const args = process.argv.slice(2);
if (process.env.FIXTURE_MODE === "compiler-stall") {
  fs.writeFileSync(process.env.FIXTURE_ROOT + "/compiler.pid", String(process.pid));
  setTimeout(() => process.exit(1), 2500);
} else {
  const built = () => {
    const output = args[args.indexOf("-o") + 1];
    fs.copyFileSync(process.env.FIXTURE_ROOT + "/browser", output);
    fs.chmodSync(output, 0o700);
  };
  if (process.env.FIXTURE_MODE === "shared-budget") setTimeout(built, 650);
  else built();
}
`;
const browser = `
const fs = require("node:fs");
const { spawn } = require("node:child_process");
const args = process.argv.slice(2);
const mode = process.env.FIXTURE_MODE;
const done = () => {
  fs.writeFileSync(args[2] + "/sample.txt", "sample");
  console.log(JSON.stringify({ done: { sent: "sample", seconds: Number(args[3]) } }));
};
if (mode === "shared-budget" || mode === "native-stall") {
  fs.writeFileSync(process.env.FIXTURE_ROOT + "/page.pid", String(process.pid));
  setTimeout(done, mode === "shared-budget" ? 650 : 2500);
} else if (mode === "close-stall") {
  const holder = spawn(process.execPath, ["-e", "setTimeout(() => {}, 2500)"],
                       { stdio: ["ignore", "inherit", "inherit"] });
  fs.writeFileSync(process.env.FIXTURE_ROOT + "/holder.pid", String(holder.pid));
  holder.unref();
  done();
} else {
  console.log("setup complete");
  done();
}
`;

async function fixture(mode, exercise) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "mac-webview-test-"));
  const restore = { ...process.env };
  try {
    for (const [name, script] of [["xcrun", compiler], ["browser", browser]]) {
      fs.writeFileSync(path.join(root, name), "#!" + process.execPath + "\n" + script, { mode: 0o700 });
    }
    Object.assign(process.env, {
      PATH: root + path.delimiter + process.env.PATH,
      TMPDIR: root,
      FIXTURE_ROOT: root,
      FIXTURE_MODE: mode,
    });
    await exercise(root);
  } finally {
    for (const key of ["PATH", "TMPDIR", "FIXTURE_ROOT", "FIXTURE_MODE"]) {
      if (restore[key] === undefined) delete process.env[key];
      else process.env[key] = restore[key];
    }
    fs.rmSync(root, { recursive: true, force: true });
  }
}

function assertExpired(ran, stage) {
  assert.equal(ran.code, 3);
  assert.equal(ran.done, undefined);
  assert.equal(ran.said.at(-1).stage, stage);
}

test("a cold build uses the page's deadline", () => fixture("compiler-stall", async (root) => {
  const ran = await runPage("fixture.html", "", 0.75);
  assertExpired(ran, "compile");
  assert.ok(fs.existsSync(path.join(root, "compiler.pid")));
}));

test("build and page share one budget", () => fixture("shared-budget", async (root) => {
  const ran = await runPage("fixture.html", "", 1.1);
  assertExpired(ran, "page");
  assert.ok(fs.existsSync(path.join(root, "page.pid")));
}));

test("a native child stuck before it prints anything times out", () => fixture("native-stall", async (root) => {
  const ran = await runPage("fixture.html", "", 0.75);
  assertExpired(ran, "page");
  assert.ok(fs.existsSync(path.join(root, "page.pid")));
}));

test("a done frame and leader exit cannot disarm a timeout with pipes still open", () => fixture("close-stall", async (root) => {
  const ran = await runPage("fixture.html", "", 0.75);
  assertExpired(ran, "page");
  assert.equal(ran.said[0].done.sent, "sample");
  const holder = fs.readFileSync(path.join(root, "holder.pid"), "utf8");
  // Give the kernel time to reap the fixture if SIGKILL arrived during exit.
  const { execFile } = require("node:child_process");
  const { setTimeout: wait } = require("node:timers/promises");
  for (let attempt = 0; attempt < 15; attempt++) {
    const stat = await new Promise((resolve) => execFile("/bin/ps", ["-p", holder, "-o", "stat="],
                                                        (_err, out) => resolve(out.trim())));
    if (!stat || stat.startsWith("Z")) return;
    await wait(20);
  }
  assert.fail("the pipe holder survived its owning WebView deadline");
}));

test("cold and cached successful runs hand back their files and done frame", () => fixture("success", async (root) => {
  for (let attempt = 0; attempt < 2; attempt++) {
    const ran = await runPage("fixture.html", "", 2);
    assert.equal(ran.code, 0);
    assert.deepEqual(ran.said[0], { text: "setup complete" });
    assert.equal(ran.done.sent, "sample");
    assert.equal(fs.readFileSync(path.join(ran.dir, "sample.txt"), "utf8"), "sample");
    if (attempt === 0) fs.unlinkSync(path.join(root, "xcrun"));
  }
}));

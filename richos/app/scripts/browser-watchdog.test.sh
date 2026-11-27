#!/usr/bin/env bash
# run-tests: no-host-screen: a stand-in WebKit and manually fired watchdog timers
# run-tests: inputs richos/app/scripts/browser-watchdog.test.sh richos/app/ui/tests/lib/browser-watchdog.js
# run-tests: covers richos/app/ui/tests/lib/browser-watchdog.js
set -euo pipefail
APP="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
exec node - "$APP/ui/tests/lib/browser-watchdog.js" <<'JS'
"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const { createRequire } = require("node:module");
const { test } = require("node:test");
const vm = require("node:vm");

const filename = process.argv[2];
const source = fs.readFileSync(filename, "utf8");
const fixtures = [
  ["page.evaluate()", (page) => page.evaluate("true")],
  ["page.screenshot()", (page) => page.screenshot({ fullPage: false })],
  ["locator.getAttribute()", (page) => page.getByRole("button").first().getAttribute("title")],
  ["locator.textContent()", (page) => page.locator("button").textContent()],
];

const methods = (names) => Object.fromEntries(names.map((name) => [name, async () => {}]));
async function verify(label, invoke) {
  const pending = new Map();
  const calls = [];
  const reports = [];
  const exits = [];
  const module = { exports: {} };
  vm.runInNewContext(source, {
    module,
    require: createRequire(filename),
    process: { stdout: { write: (_text, done) => done() }, exit: (code) => exits.push(code) },
    setTimeout(fn, ms) {
      const key = Symbol();
      pending.set(key, { fn, ms });
      return key;
    },
    clearTimeout: (key) => pending.delete(key),
  }, { filename });

  let stall = false;
  const never = new Promise(() => {});
  function reply(...args) {
    assert.ok(this.kind, "the Playwright method keeps its receiver");
    calls.push(args);
    return stall ? never : Promise.resolve("fixture reply");
  }

  const factories = ["locator", "getByRole", "getByText", "getByLabel", "getByTestId"];
  function locator() {
    const target = {
      kind: "locator",
      ...methods(["count", "allTextContents", "evaluate", "evaluateAll", "isDisabled", "all"]),
      getAttribute: reply, textContent: reply,
    };
    for (const name of [...factories, "filter", "first", "last", "nth", "and", "or"]) target[name] = locator;
    return target;
  }
  const page = {
    kind: "page",
    ...methods(["addInitScript", "addStyleTag", "close", "emulateMedia", "setViewportSize", "waitForFunction"]),
    evaluate: reply, screenshot: reply,
    clock: methods(["setFixedTime"]),
    keyboard: methods(["press", "type"]),
    mouse: methods(["click", "dblclick", "down", "move", "up", "wheel"]),
  };
  for (const name of factories) page[name] = locator;
  const webkit = { launch: async () => ({ ...methods(["close", "newContext"]), newPage: async () => page }) };
  const run = {
    async check(_name, work) {
      try {
        await work();
      } catch (error) {
        reports.push(error.message);
      }
    },
    report() { reports.push("reported"); },
  };
  const browser = await module.exports.launchGuardedWebKit(webkit, run);
  const guarded = await browser.newPage();
  const immediate = invoke(guarded);
  assert.deepEqual([...pending.values()].map(({ ms }) => ms), [60_000], label);
  assert.equal(await immediate, "fixture reply");
  assert.equal(pending.size, 0);

  stall = true;
  const checking = run.check("a stalled operation", () => invoke(guarded));
  assert.deepEqual([...pending.values()].map(({ ms }) => ms), [60_000], label);
  const [key, { fn }] = pending.entries().next().value;
  pending.delete(key);
  fn();
  await checking;
  assert.deepEqual(reports, [`WebKit did not answer ${label} within 60s`, "reported"]);
  assert.deepEqual(exits, [1]);
  assert.deepEqual(calls[0], calls[1]);
}

for (const [label, invoke] of fixtures) test(label, () => verify(label, invoke));
JS

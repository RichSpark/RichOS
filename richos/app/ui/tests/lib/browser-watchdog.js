"use strict";

const fs = require("node:fs");

// Playwright bounds navigation and action waits, but calls such as evaluate() and
// close() can wait indefinitely for a WebKit reply. Bound those operations on the
// Node side so a slow check with several successful calls keeps making progress.
const REPLY_LIMIT_MS = 60_000;
const LOCATOR_FACTORIES = ["locator", "getByRole", "getByText", "getByLabel", "getByTestId"];

async function launchGuardedWebKit(webkit, run) {
  let abandonCheck = null;
  const runCheck = run.check.bind(run);
  run.check = async (name, work) => {
    let stalled = false;
    const abandoned = new Promise((_, reject) => {
      abandonCheck = (error) => {
        stalled = true;
        reject(error);
      };
    });
    try {
      await runCheck(name, () => Promise.race([abandoned, work()]));
    } finally {
      abandonCheck = null;
    }
    if (stalled) {
      run.report();
      // Give the results and failure reason to the runner before the browser reaper takes over.
      await new Promise((resolve) => process.stdout.write("", resolve));
      process.exit(1);
    }
  };

  async function awaitReply(operation, work) {
    const timer = setTimeout(() => {
      const error = new Error(`WebKit did not answer ${operation} within ${REPLY_LIMIT_MS / 1000}s`);
      if (abandonCheck) {
        abandonCheck(error);
      } else {
        fs.writeSync(2, error.message + "\n");
        process.exit(1);
      }
    }, REPLY_LIMIT_MS);
    try {
      return await work();
    } finally {
      clearTimeout(timer);
    }
  }

  function guardCalls(target, type, methods, wrapResult = (result) => result) {
    for (const method of methods) {
      const call = target[method].bind(target);
      target[method] = async (...args) =>
        wrapResult(await awaitReply(`${type}.${method}()`, () => call(...args)));
    }
  }

  function guardLocatorFactories(target, methods) {
    for (const method of methods) {
      const find = target[method].bind(target);
      target[method] = (...args) => guardLocator(find(...args));
    }
  }

  function guardLocator(locator) {
    guardCalls(locator, "locator", [
      "count", "allTextContents", "evaluate", "evaluateAll", "getAttribute", "isDisabled", "textContent",
    ]);
    guardCalls(locator, "locator", ["all"], (locators) => locators.map(guardLocator));
    guardLocatorFactories(locator, [...LOCATOR_FACTORIES, "filter", "first", "last", "nth", "and", "or"]);
    return locator;
  }

  const pages = new WeakSet();
  function guardPage(page) {
    if (pages.has(page)) return page;
    pages.add(page);
    guardCalls(page, "page", [
      "addInitScript", "addStyleTag", "close", "emulateMedia", "evaluate", "screenshot", "setViewportSize",
    ]);
    // Playwright's waitForFunction timeout tries to cancel the poll in the page.
    // An unresponsive renderer can leave that cancellation waiting for a reply.
    guardCalls(page, "page", ["waitForFunction"]);
    guardCalls(page.clock, "page.clock", ["setFixedTime"]);
    guardCalls(page.keyboard, "page.keyboard", ["press", "type"]);
    guardCalls(page.mouse, "page.mouse", ["click", "dblclick", "down", "move", "up", "wheel"]);
    guardLocatorFactories(page, LOCATOR_FACTORIES);
    return page;
  }

  function guardContext(context) {
    guardCalls(context, "context", ["close"]);
    guardCalls(context, "context", ["newPage"], guardPage);
    return context;
  }

  const browser = await awaitReply("launch()", () => webkit.launch());
  guardCalls(browser, "browser", ["close"]);
  guardCalls(browser, "browser", ["newContext"], guardContext);
  guardCalls(browser, "browser", ["newPage"], guardPage);
  return browser;
}

module.exports = { launchGuardedWebKit };

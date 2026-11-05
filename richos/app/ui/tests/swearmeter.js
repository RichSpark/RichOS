// SwearMeter: cached reveal over a conversation, hidden updates, ordered results,
// durable display preference, qualified partial history and failed saves.
// Runs the real renderer and settings shell in headless WebKit. All data is synthetic.
"use strict";
const path = require("path");
const { loadPlaywright, createRun, assert, assertEqual, UI_DIR, leaveHome, bootSettled, captureSettled, publishShot } = require("./lib/harness");
const APP = "file://" + path.join(UI_DIR, "index.html");
async function openApp(context, initialDelay = false) {
  const page = await context.newPage();
  page.errors = [];
  page.on("pageerror", error => page.errors.push(String(error)));
  await page.addInitScript(({ initialDelay }) => {
    const result = { detector_version: 1, count: 17, unknown_card_inputs: 0, unknown_message_inputs: 0, incomplete_history: false, available: true, revision: 1 };
    window.calls = [];
    window.fixture = { result, failSave: false, initialDelay };
    Object.defineProperty(window, "RichBridge", {
      configurable: true,
      set(bridge) {
        const invoke = bridge.invoke.bind(bridge), listen = bridge.listen.bind(bridge);
        bridge.listen = (name, callback) => {
          if (name === "rich://swearmeter") window.deliverSwearMeter = value => callback({ payload: value });
          return listen(name, callback);
        };
        bridge.invoke = async (name, args) => {
          if (name.includes("swearmeter")) window.calls.push(name);
          if (name === "get_swearmeter") {
            const initial = await invoke(name, args);
            if (window.fixture.initialDelay) await new Promise(resolve => { window.finishInitial = resolve; });
            return { show: initial.show, result: window.fixture.result };
          }
          if (name === "set_swearmeter_visible" && window.fixture.failSave) throw Error("fixture disk refused write");
          if (name === "retry_swearmeter") return { ...result, count: 25, revision: 20 };
          return invoke(name, args);
        };
        Object.defineProperty(window, "RichBridge", { value: bridge, writable: true, configurable: true });
      }
    });
  }, { initialDelay });
  await page.goto(APP);
  await leaveHome(page);
  await bootSettled(page);
  await page.evaluate(() => openThread("general"));
  await page.click("#set-btn");
  await page.waitForSelector("#set-swearmeter");
  return page;
}
const rowText = page => page.locator("#set-swearmeter-result").textContent();
async function main() {
  const run = createRun("SwearMeter maintained display");
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();
  const context = await browser.newContext({ viewport: { width: 1440, height: 1000 }, colorScheme: "dark" });
  let page = await openApp(context);
  await run.check("1 hidden by default; reveal is synchronous and starts no counting or history read", async () => {
    assertEqual(await page.locator("#set-swearmeter").isChecked(), false, "default off");
    assertEqual(await rowText(page), "", "no hidden number");
    const reveal = await page.evaluate(() => {
      window.calls.length = 0;
      const started = performance.now();
      document.getElementById("set-swearmeter").click();
      return { elapsed: performance.now() - started, text: document.getElementById("set-swearmeter-result").textContent,
        calls: [...window.calls], thread: activeThreadId, settingsOpen: document.getElementById("set-btn").getAttribute("aria-expanded") };
    });
    assertEqual(reveal.text, "SwearMeter: 17", "cached result on switch event");
    assertEqual(reveal.calls, ["set_swearmeter_visible"], "only save display preference");
    // Record elapsed time as evidence; correctness is the cached text in this event.
    // Host load must not turn a timing target into a flaky assertion.
    assertEqual([reveal.thread, reveal.settingsOpen], ["general", "true"], "conversation and settings stay open");
    return JSON.stringify(reveal);
  });
  await run.check("2 hidden updates keep counting; stale notifications cannot overwrite newer results", async () => {
    await page.click("#set-swearmeter");
    await page.evaluate(() => {
      deliverSwearMeter({ ...fixture.result, count: 23, revision: 3 });
      deliverSwearMeter({ ...fixture.result, count: 18, revision: 2 });
    });
    assertEqual(await rowText(page), "", "hidden updated count remains undisplayed");
    await page.click("#set-swearmeter");
    assertEqual(await rowText(page), "SwearMeter: 23", "reveal latest maintained result");
    const home = await page.evaluate(() => {
      const node = document.getElementById("home-swearmeter");
      return { text: node.textContent, last: node === document.getElementById("home-signals").lastElementChild };
    });
    assert(home.text.includes("SwearMeter: 23") && home.text.includes("English dictionary"), "home shows same scope and value");
    assert(home.last, "SwearMeter must sit after positive signals");
  });
  await run.check("3 partial zero remains qualified and unavailable never looks like zero", async () => {
    await page.evaluate(() => deliverSwearMeter({ ...fixture.result, count: 0, unknown_card_inputs: 2, unknown_message_inputs: 1, revision: 4 }));
    const partial = await rowText(page);
    assert(partial.includes("SwearMeter: 0") && partial.includes("Partial history") && partial.includes("2 records"), "qualified zero");
    assert((await page.locator("#home-swearmeter").textContent()).includes("Partial history"), "home carries coverage too");
    await page.evaluate(() => deliverSwearMeter({ ...fixture.result, available: false, count: 0, revision: 5 }));
    assert((await rowText(page)).startsWith("SwearMeter unavailable"), "no false zero");
    await page.click("#set-swearmeter-retry");
    assertEqual(await rowText(page), "SwearMeter: 25", "explicit recovery");
  });
  await run.check("4 failed preference write restores the saved display choice and counting continues", async () => {
    await page.evaluate(() => { fixture.failSave = true; });
    await page.click("#set-swearmeter");
    await page.waitForFunction(() => document.getElementById("set-swearmeter").checked);
    assert((await page.locator("#bug-toast").textContent()).includes("not saved"), "honest save failure");
    await page.evaluate(() => deliverSwearMeter({ ...fixture.result, count: 26, revision: 21 }));
    assertEqual(await rowText(page), "SwearMeter: 26", "updates still arrive");
    await page.evaluate(() => { fixture.failSave = false; });
  });
  await run.check("5 preference survives a relaunch; keyboard toggling hides both results", async () => {
    await page.close();
    page = await openApp(context);
    assert(await page.locator("#set-swearmeter").isChecked(), "saved display choice");
    await page.locator("#set-swearmeter").focus();
    await page.keyboard.press("Space");
    assertEqual(await rowText(page), "", "settings hides number and qualifier");
    assertEqual(await page.locator("#home-swearmeter").count(), 0, "hidden home display adds no empty signal node");
    assertEqual(page.errors, [], "no renderer errors");
  });
  await run.check("6 a late initial snapshot cannot undo an event or a more recent display choice", async () => {
    await page.close();
    page = await openApp(context, true);
    await page.evaluate(() => deliverSwearMeter({ ...fixture.result, count: 99, revision: 40 }));
    await page.click("#set-swearmeter");
    await page.evaluate(() => finishInitial());
    await page.waitForFunction(() => !document.getElementById("set-swearmeter").disabled);
    assert(await page.locator("#set-swearmeter").isChecked(), "initial preference did not undo switch");
    assertEqual(await rowText(page), "SwearMeter: 99", "initial result did not undo latest event");
    assertEqual(page.errors, [], "no renderer errors");
  });
  await run.check("7 results remain readable in light and dark settings", async () => {
    await page.evaluate(() => deliverSwearMeter({ ...fixture.result, count: 1234, unknown_card_inputs: 3, revision: 41 }));
    for (const theme of ["light", "dark"]) {
      await page.evaluate(theme => RichTheme.setTheme(theme), theme);
      const geometry = await page.locator("#set-swearmeter-block").evaluate(node => {
        const box = node.getBoundingClientRect();
        const result = node.querySelector("#set-swearmeter-result");
        return { width: box.width, textWidth: result.scrollWidth, available: result.clientWidth,
          color: getComputedStyle(result).color, checkbox: node.querySelector("input").getBoundingClientRect().width };
      });
      assert(geometry.width > 100 && geometry.checkbox > 0, "settings row is laid out");
      assert(geometry.textWidth <= geometry.available + 1, "result must wrap within settings column");
      if (process.env.RICHOS_SWEARMETER_EVIDENCE_DIR) {
        const fs = require("fs");
        fs.mkdirSync(process.env.RICHOS_SWEARMETER_EVIDENCE_DIR, { recursive: true });
        publishShot(await captureSettled(page), path.join(process.env.RICHOS_SWEARMETER_EVIDENCE_DIR, "settings-" + theme + ".png"));
      }
    }
  });
  await run.check("8 home places the maintained result below the positive statistics", async () => {
    await page.evaluate(() => { RichSettings.close(); RichHome.show(); });
    await page.waitForSelector("#home-swearmeter", { state: "visible" });
    await page.waitForFunction(() => !document.getElementById("home").classList.contains("home-leaving") && getComputedStyle(document.getElementById("home")).opacity === "1");
    const layout = await page.evaluate(() => {
      const meter = document.getElementById("home-swearmeter");
      const previous = meter.previousElementSibling;
      return { y: meter.getBoundingClientRect().top, previousBottom: previous?.getBoundingClientRect().bottom,
        text: meter.textContent, width: meter.clientWidth, scroll: meter.scrollWidth };
    });
    assert(layout.previousBottom <= layout.y, "result follows positive statistics vertically");
    assert(layout.text.includes("1,234") && layout.text.includes("Partial history"), "home shows current qualified result");
    assert(layout.scroll <= layout.width + 1, "home explanation wraps");
    if (process.env.RICHOS_SWEARMETER_EVIDENCE_DIR) {
      for (const theme of ["light", "dark"]) {
        await page.evaluate(theme => RichTheme.setTheme(theme), theme);
        publishShot(await captureSettled(page), path.join(process.env.RICHOS_SWEARMETER_EVIDENCE_DIR, "home-" + theme + ".png"));
      }
    }
  });
  await run.check("9 a late saved preference is restored after an early failed switch write", async () => {
    await page.close();
    // Earlier checks left the durable preference enabled.
    page = await openApp(context, true);
    await page.evaluate(() => { fixture.failSave = true; });
    await page.click("#set-swearmeter");
    await page.waitForFunction(() => !document.getElementById("set-swearmeter").disabled);
    await page.evaluate(() => finishInitial());
    await page.waitForFunction(() => document.getElementById("set-swearmeter").checked);
    assertEqual(await rowText(page), "SwearMeter: 17", "durable preference wins after failed write");
    assertEqual(page.errors, [], "no renderer errors");
  });
  await context.close();
  await browser.close();
  return run.report();
}
main().then(failed => { process.exitCode = failed ? 1 : 0; });

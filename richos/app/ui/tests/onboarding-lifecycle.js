// First-client regressions: the onboarding offer must preserve drafts and stay bound to
// the company whose state/action it represents while bridge calls are in flight.
"use strict";
const { loadPlaywright, createRun, assert, assertEqual } = require("./lib/harness");
const { openApp, noticeText } = require("./onboarding");

async function main() {
  const run = createRun("onboarding lifecycle: drafts, partial answers and company races");
  const browser = await loadPlaywright().webkit.launch();
  try {
    await run.check("Start sends only the acceptance and preserves an existing draft", async () => {
      const page = await openApp(browser, { onboarding: "not-yet" });
      await page.waitForSelector("#first-run:not([hidden])");
      await page.fill("#input", "My unsent notes about the board meeting");
      await page.evaluate(() => {
        const original = window.RichBridge.invoke.bind(window.RichBridge);
        window.__sent = [];
        window.RichBridge.invoke = (cmd, args) => {
          if (cmd === "send_message") {
            window.__sent.push(args.text);
            return new Promise(() => {});
          }
          return original(cmd, args);
        };
      });
      await page.click("#first-run-start");
      // A duplicate queued UI activation cannot create a second acceptance.
      await page.dispatchEvent("#first-run-start", "click");
      assertEqual(await page.evaluate(() => window.__sent), ["Let's do the onboarding."]);
      assertEqual(await page.inputValue("#input"), "My unsent notes about the board meeting");
      await page.evaluate(() => window.__RICHOS_FIRST_RUN__());
      assert(await page.isHidden("#first-run"), "a refresh cannot reopen the offer during the accepted action");
      await page.close();
    });

    await run.check("a company Rich added during his turn redraws the company list when the app adopts it", async () => {
      const page = await openApp(browser, { onboarding: "partial" });
      await page.waitForSelector("#first-run:not([hidden])");
      await page.evaluate(() => { window.__calls = []; });
      await page.evaluate(() => window.__emit("rich://companies-changed", null));
      const cmds = await page.evaluate(() => window.__calls.map((c) => c.cmd));
      assert(cmds.includes("entity_choice") && cmds.includes("navigation_tree"),
        "the company setting and the rail are read again: " + JSON.stringify(cmds));
      await page.close();
    });

    await run.check("partial answers offer Resume and send a continuation, in both themes", async () => {
      for (const theme of ["light", "dark"]) {
        const page = await openApp(browser, { onboarding: "partial" }, theme);
        await page.waitForSelector('#first-run[data-state="partial"]:not([hidden])');
        assert((await noticeText(page)).includes("What you've shared is kept"));
        assertEqual((await page.locator("#first-run-start").textContent()).trim(), "Resume onboarding");
        await page.click("#first-run-start");
        await page.waitForFunction(() => window.__calls.some(c => c.cmd === "send_message"));
        assertEqual(await page.evaluate(() => window.__calls.find(c => c.cmd === "send_message").args.text),
          "Let's pick up onboarding where we stopped.");
        await page.close();
      }
    });

    await run.check("Not now binds the displayed company and disables both conflicting actions", async () => {
      const page = await openApp(browser, { onboarding: "not-yet" });
      await page.waitForSelector("#first-run:not([hidden])");
      const entityId = await page.evaluate(async () => (await window.RichBridge.invoke("onboarding_view")).entityId);
      await page.evaluate(() => {
        const original = window.RichBridge.invoke.bind(window.RichBridge);
        window.__declines = [];
        window.RichBridge.invoke = (cmd, args) => {
          if (cmd === "decline_onboarding") {
            window.__declines.push(args);
            return new Promise(resolve => { window.__finishDecline = resolve; });
          }
          return original(cmd, args);
        };
      });
      await page.click("#first-run-later");
      assert(await page.isDisabled("#first-run-start"));
      assert(await page.isDisabled("#first-run-later"));
      await page.dispatchEvent("#first-run-start", "click");
      assertEqual(await page.evaluate(() => window.__declines), [{ entityId }]);
      assertEqual(await page.evaluate(() => window.__calls.filter(c => c.cmd === "send_message").length), 0);
      await page.evaluate(entityId => window.__finishDecline({ entityId, state: "declined" }), entityId);
      await page.waitForSelector('#first-run[data-state="declined"]');
      await page.close();
    });

    await run.check("an older state read cannot overwrite the result of Not now", async () => {
      const page = await openApp(browser, { onboarding: "not-yet" });
      await page.waitForSelector("#first-run:not([hidden])");
      await page.evaluate(() => {
        const original = window.RichBridge.invoke.bind(window.RichBridge);
        let delayNext = true;
        window.RichBridge.invoke = async (cmd, args) => {
          const result = await original(cmd, args);
          if (cmd === "onboarding_view" && delayNext) {
            delayNext = false;
            return new Promise(resolve => { window.__oldRead = () => resolve(result); });
          }
          return result;
        };
        window.__RICHOS_FIRST_RUN__();
      });
      await page.waitForFunction(() => typeof window.__oldRead === "function");
      await page.click("#first-run-later");
      await page.waitForSelector('#first-run[data-state="declined"]');
      await page.evaluate(() => window.__oldRead());
      await page.waitForTimeout(100);
      assertEqual(await page.locator("#first-run").getAttribute("data-state"), "declined");
      assert(await page.isHidden("#first-run-actions"));
      await page.close();
    });

    await run.check("a delayed decline receipt cannot replace a different company's offer", async () => {
      const page = await openApp(browser, { onboarding: "not-yet" });
      await page.waitForSelector("#first-run:not([hidden])");
      await page.evaluate(() => {
        const original = window.RichBridge.invoke.bind(window.RichBridge);
        window.RichBridge.invoke = async (cmd, args) => {
          const result = await original(cmd, args);
          if (cmd === "decline_onboarding") return new Promise(resolve => { window.__oldDecline = () => resolve(result); });
          return result;
        };
      });
      await page.click("#first-run-later");
      await page.waitForFunction(() => typeof window.__oldDecline === "function");
      await page.click('.nav-thread[data-thread-id="acme"]');
      await page.waitForFunction(() => window.__RICHOS_TIMELINE__().threadId === "acme");
      await page.waitForSelector('#first-run[data-state="not-yet"]:not([hidden])');
      await page.evaluate(() => window.__oldDecline());
      await page.waitForTimeout(100);
      assertEqual(await page.locator("#first-run").getAttribute("data-state"), "not-yet");
      assert(!(await page.isDisabled("#first-run-start")), "the new company's action is available");
      assert(!(await noticeText(page)).includes("Left with you"));
      await page.close();
    });
  } finally { await browser.close(); }
  process.exitCode = run.report() ? 1 : 0;
}
main().catch(error => { console.error(error); process.exitCode = 1; });

"use strict";
// Renderer conformance with a simulated provider. Real browser authorization is
// an installed acceptance case and must use the provider's own flow.
const path = require("path");
const {loadPlaywright, createRun, assert, assertEqual, UI_DIR, leaveHome} = require("./lib/harness");
async function open(browser, preset) {
  const page = await browser.newPage({viewport: {width: 1280, height: 900}});
  const errors = [];
  page.on("pageerror", e => errors.push(String(e)));
  await page.addInitScript(preset => {
    window.__RICHOS_MOCK_PRESET__ = preset;
    window.__calls = [];
    let bridge;
    Object.defineProperty(window, "RichBridge", {configurable: true, get: () => bridge, set: value => {
      bridge = value;
      const invoke = value.invoke.bind(value);
      value.invoke = (cmd, args) => { window.__calls.push({cmd, args}); return invoke(cmd, args); };
    }});
  }, preset);
  await page.goto("file://" + path.join(UI_DIR, "index.html"));
  await leaveHome(page);
  page.errors = errors;
  return page;
}
async function main() {
  const run = createRun("provider account connection in the shipping renderer");
  const browser = await loadPlaywright().webkit.launch();
  await run.check("signed-out first launch offers browser login and verifies return", async () => {
    const page = await open(browser, {providerAuth: "signed-out"});
    await page.waitForSelector("#provider-connect:not([hidden])");
    assert(await page.isHidden("#setup-go"), "already installed software must not be reinstalled to log in");
    await page.click("#provider-connect");
    await page.waitForFunction(() => document.getElementById("setup-account").textContent.includes("is connected"));
    assert(await page.isHidden("#provider-connect"), "connected account still offered login");
    assert(await page.isHidden("#provider-cancel"), "completed login still offered cancellation");
    const calls = await page.evaluate(() => window.__calls.filter(c => c.cmd === "provider_auth_start"));
    assertEqual(calls.length, 1, "login count");
    assertEqual(Object.keys(calls[0].args || {}).length, 0, "sign-in carries no account-type argument");
    assert(await page.evaluate(() => !document.getElementById("provider-account-kind") && !document.getElementById("provider-account-select")), "the Account type menu is back");
    assertEqual(page.errors.length, 0, "renderer errors");
    await page.close();
    return "browser handoff, verified return and one login request";
  });
  await run.check("cancel permits a later retry without dismissing an active login", async () => {
    const page = await open(browser, {providerAuth: "signed-out", providerAuthHold: true});
    await page.waitForSelector("#provider-connect:not([hidden])");
    await page.click("#provider-connect");
    await page.waitForSelector("#provider-cancel:not([hidden])");
    assert(await page.isHidden("#setup-close"), "active login can disappear without cancellation");
    await page.click("#provider-cancel");
    await page.waitForSelector("#provider-connect:not([hidden])");
    assert((await page.textContent("#setup-account")).includes("canceled"), "cancellation not reported");
    await page.evaluate(() => { window.__RICHOS_MOCK_PRESET__.providerAuthHold = false; });
    await page.click("#provider-connect");
    await page.waitForFunction(() => document.getElementById("setup-account").textContent.includes("is connected"));
    const calls = await page.evaluate(() => window.__calls.filter(c => c.cmd === "provider_auth_start"));
    assertEqual(calls.length, 2, "one request per explicit attempt");
    assertEqual(Object.keys(calls[1].args || {}).length, 0, "retry carries no account-type argument");
    assertEqual(page.errors.length, 0, "renderer errors");
    await page.close();
    return "explicit cancellation and retry select the intended provider flow";
  });
  await run.check("connected account stays quiet and settings can reopen connection", async () => {
    const page = await open(browser, {});
    await page.waitForSelector(".nav-thread", {state: "attached"});
    assert(await page.isHidden("#setup-sheet"), "connected account interrupted launch");
    // Since round 18 (Claude accounts in Settings), the door is the first account's card.
    await page.click("#set-btn");
    await page.click("#set-accounts-open");
    await page.click('#accounts-sheet [data-act="sign-one"]');
    await page.waitForFunction(() => document.getElementById("setup-account").textContent.includes("is connected"));
    assert(await page.isVisible("#setup-sheet"), "settings did not open connection");
    assertEqual(page.errors.length, 0, "renderer errors");
    await page.close();
    return "no first-run interruption and a working settings entry";
  });
  await browser.close();
  process.exitCode = run.report() ? 1 : 0;
}
main().catch(error => { console.error(error); process.exit(1); });

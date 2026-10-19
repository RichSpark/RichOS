"use strict";
// DICTATION: THE SETTINGS ROW AND THE DICTATION SHEET (round 19, states 5 to 12, and round 19's
// more lines 1, 2 and 4; richos-hq design/mockups/rounds/round-19/dictation.html and
// dictation-more-lines.html), built to slice 2 of the dictation plan (richos-hq
// docs/plans/2026-10-08-dictation-anywhere.md revision 2, section 9). Measured against the CEO's
// words: "it all looks awesome." and ""On in another copy of RichOS" is confusing as hell. Change
// to: "On even when RichOS is closed"." (2026-10-08).
//
// One check per part of the slice, each red on main (where there is no row, no sheet, no
// `dictation_status` and no `window.RichDictation`): the gate, the row's states, every sheet
// state, turning it on in the drawn order, the refusals, the key and its refusals, accuracy,
// the three more lines, the off notice, Rich's offer's turnOn, Escape; then every state in both
// themes for contrast, the type floor, fit and no dash, computed in WebKit.
const path = require("path");
const contrast = require("./lib/contrast");
const { leaveHome, loadPlaywright, createRun, assert, assertEqual, UI_DIR } = require("./lib/harness");

const ALLOWED = { on: true, mic: "allowed", ax: "allowed", owner: "self", keyTap: true };

async function main() {
  const run = createRun("Dictation: the Settings row and the sheet (round 19, slice 2)");
  const browser = await loadPlaywright().webkit.launch();
  const errors = [];
  async function open(theme, view, opts = {}) {
    const page = await browser.newPage({ viewport: opts.viewport || { width: 1440, height: 900 }, locale: "en-US" });
    page.setDefaultTimeout(30000); // load-bound: a hang guard only; every wait below waits for a fact
    page.on("pageerror", e => errors.push(String(e)));
    await page.addInitScript(({ theme, view, ready, axAsked, tapNever, sheetPending, owner, extra }) => {
      localStorage.setItem("richos-theme", theme);
      localStorage.setItem("richos-mock-config", JSON.stringify({ theme, font_scale: 100 }));
      window.__RICHOS_MOCK_PRESET__ = { dictation: ready, dictationView: view, dictationAxAsked: axAsked, dictationTapNever: tapNever, dictationSheetPending: sheetPending, dictationOwner: owner, ...extra };
    }, { theme, view: view || {}, ready: opts.ready !== false, axAsked: !!opts.axAsked, sheetPending: !!opts.sheetPending, owner: opts.owner, extra: opts.preset || {},
         // A fixture that says keyTap false with its own tool and Accessibility allowed is a tool
         // whose tap never comes (the sheet pokes it once; recheck finding 6), unless opts says.
         tapNever: opts.tapNever !== undefined ? !!opts.tapNever : !!(view && view.keyTap === false && view.owner === "self" && view.ax === "allowed") });
    await page.goto("file://" + path.join(UI_DIR, "index.html"));
    await page.waitForSelector("#set-btn");
    await page.evaluate(() => window.RichSplash.yieldNow("dictation-test"));
    await page.evaluate(t => document.documentElement.setAttribute("data-theme", t), theme);
    return page;
  }
  async function menu(page) {
    await page.click("#set-btn");
    await page.waitForSelector("#set-dictation-open");
    await page.waitForFunction(() => document.getElementById("set-dictation-state").textContent.length > 0);
  }
  async function sheet(page) {
    await menu(page);
    await page.click("#set-dictation-open");
    await page.waitForSelector("#dictation-sheet:not([hidden]) #dict-state");
  }
  const text = (page, sel) => page.locator(sel).innerText().then(t => t.replace(/\s+/g, " ").trim());
  const calls = page => page.evaluate(() => window.__RICHOS_MOCK__.dictationCalls().map(c => c.cmd + (c.on !== undefined ? ":" + c.on : "") + (c.key !== undefined ? ":" + c.key : "") + (c.accuracy ? ":" + c.accuracy : "") + (c.pane ? ":" + c.pane : "") + (c.forward !== undefined ? ":" + c.forward : "")).filter(c => c !== "dictation_status" && c !== "dictation_take_sheet_request" && c !== "dictation_feature_thread"));
  const set = (page, patch) => page.evaluate(p => window.__RICHOS_MOCK__.dictationSet(p), patch);
  const waitText = (page, sel, want) => page.waitForFunction(([s, w]) => {
    const n = document.querySelector(s);
    return n && n.innerText.replace(/\s+/g, " ").trim() === w;
  }, [sel, want]);

  // ---- 1. the gate ------------------------------------------------------------------------
  await run.check("the gate: with dictation not there (DICTATION_READY false) Settings has no Dictation row and the key says nothing", async () => {
    const page = await open("dark", {}, { ready: false });
    await page.click("#set-btn");
    await page.waitForSelector("#set-menu:not([hidden])");
    await page.waitForFunction(() => window.__RICHOS_MOCK__.dictationCalls().length > 0);
    assertEqual(await page.locator("#set-dictation-open").count(), 0, "no row");
    await page.keyboard.press("Escape");
    await page.keyboard.press("F1");
    assert(await page.locator("#bug-toast:not([hidden])").count() === 0, "no notice about a feature that is not there");
    assert(await page.locator("#dictation-sheet").isHidden(), "the sheet stays shut");
    await page.close();
    return "no row, no notice";
  });

  // ---- 2. the row ---------------------------------------------------------------------------
  await run.check("the row: Dictation after Claude accounts, with round 19's line for each state and the more lines' words", async () => {
    const rows = [
      [{}, "Off", false],
      [ALLOWED, "On. Tap F1 to talk", false],
      [{ ...ALLOWED, key: 5 }, "On. Tap F5 to talk", false],
      [{ on: true, mic: "denied", ax: "unknown" }, "Needs a permission", true],
      [{ on: true, mic: "asking", ax: "unknown" }, "Waiting for macOS", true],
      [{ ...ALLOWED, owner: "other" }, "On. Tap F1 to talk", false],
      [{ ...ALLOWED, owner: "none", keyTap: false }, "Starting", false],
      [{ ...ALLOWED, keyTap: false }, "Not working yet", true],
      [{ ...ALLOWED, secure: { app: "1Password" } }, "Paused: 1Password is hiding your keys", true],
      [{ ...ALLOWED, secure: { app: null } }, "Paused: an app is hiding your keys", true],
    ];
    const seen = [];
    for (const [view, want, attention] of rows) {
      const page = await open("dark", view);
      await menu(page);
      await waitText(page, "#set-dictation-state", want);
      assert((await page.textContent("#set-dictation-open")).trim().startsWith("RichScribe"), "the row is named RichScribe (the CEO, 2026-10-19)");
      assertEqual(await page.locator("#set-dictation-state.is-attention").count(), attention ? 1 : 0, want + ": attention");
      // Round 27: under "Working with Rich", after the interruption choice and directly above
      // Use Rich from your phone, as drawn.
      const order = await page.locator("#set-menu button.bugbtn").evaluateAll(b => b.map(x => x.id));
      assert(order.indexOf("set-dictation-open") + 1 === order.indexOf("set-phone-open"), "directly above Use Rich from your phone, as drawn: " + order);
      assertEqual(await page.evaluate(() => document.getElementById("set-dictation-open").closest(".sm-col").querySelector(".sm-group").textContent), "Working with Rich", "in the Working with Rich column");
      seen.push(want);
      await page.close();
    }
    return seen.join(" / ");
  });

  // ---- 3. the sheet, off --------------------------------------------------------------------
  await run.check("state 6: the sheet, off: the switch, Your key with F1 right of esc, What macOS asks you for, Accuracy, Try it here, and line 2", async () => {
    const page = await open("dark", {});
    await sheet(page);
    assertEqual(await text(page, "#dict-title"), "RichScribe");
    assertEqual(await text(page, ".dict-lede"), "Type with your voice in any app on your Mac. Your words appear where your cursor is.");
    assertEqual(await text(page, ".dict-card-title"), "Dictate in any app");
    assertEqual(await text(page, "#dict-state"), "Off. Turn it on to type with your voice in Mail, Slack, your browser, anywhere.");
    assertEqual(await page.getAttribute("#dict-switch", "aria-checked"), "false");
    assertEqual(await text(page, "#dict-copy-note"), "Works only while RichOS is open. When you quit RichOS, RichScribe stops until you open it again.");
    const keys = await page.locator(".dict-keys > *").allInnerTexts();
    assertEqual(keys, ["esc", "F1", "F2", "F3", "F4", "F5", "F6", "F7"], "esc, then F1 to F7");
    assertEqual(await page.getAttribute('[data-act="key"][data-k="1"]', "aria-checked"), "true");
    assertEqual(await text(page, ".dict-key-row .dict-key-say"), "F1 is the key just right of esc. Pick one you rarely use.");
    assertEqual(await page.locator(".dict-perm-state").allInnerTexts(), ["Asked when you turn it on", "Asked when you turn it on"]);
    assertEqual(await text(page, '[data-perm="microphone"] .dict-perm-s'), "So I can hear you, only while you dictate.");
    assertEqual(await text(page, '[data-perm="accessibility"] .dict-perm-s'), "So I can type your words where your cursor is, in any app.");
    assertEqual(await page.getAttribute('[data-act="accuracy"][data-v="accurate"]', "aria-checked"), "true");
    assertEqual(await page.getAttribute("#dict-try", "data-ph"), "Turn RichScribe on to try it here.");
    assert(await page.locator("#dict-try-note").isHidden(), "the try note waits for dictation to work");
    assertEqual(await text(page, ".dict-priv"), "Your voice stays on this Mac. Nothing you say is sent anywhere.");
    await page.close();
    // Line 2 for every copy (the CEO, 2026-10-08: dictation runs only while RichOS runs), on or
    // off, and for a copy whose key another copy holds.
    const on = await open("dark", ALLOWED);
    await sheet(on);
    assertEqual(await text(on, "#dict-copy-note"), "Works only while RichOS is open. When you quit RichOS, RichScribe stops until you open it again.");
    await on.close();
    const other = await open("dark", { ...ALLOWED, owner: "other" });
    await sheet(other);
    assertEqual(await text(other, "#dict-copy-note"), "Works only while RichOS is open. When you quit RichOS, RichScribe stops until you open it again.");
    await other.close();
    return "off, and line 2 for every copy";
  });

  // ---- 4. turning it on ------------------------------------------------------------------------
  await run.check("states 7 to 10: the switch asks for the microphone, then Accessibility; allowed in System Settings, the sheet says On with no relaunch", async () => {
    const page = await open("dark", {});
    await sheet(page);
    await page.click("#dict-switch");
    await waitText(page, '[data-perm="microphone"] .dict-perm-state', "macOS is asking you");
    assertEqual(await text(page, "#dict-state"), "Waiting for you to allow the microphone…");
    // State 7 as drawn: only the microphone is being asked; Accessibility comes after it (the
    // first guest walk, walk-63fc2c726051, showed both rows "macOS is asking you").
    assertEqual(await text(page, '[data-perm="accessibility"] .dict-perm-state'), "Asked when you turn it on");
    assertEqual(await page.evaluate(() => document.getElementById("set-dictation-state").textContent), "Waiting for macOS", "the row says so too");
    assertEqual(await page.getAttribute("#dict-switch", "aria-checked"), "true");
    assertEqual(await calls(page), ["dictation_set_on:true", "dictation_ask_microphone"]);
    // macOS answered Allow: the flow goes on to Accessibility, by itself.
    await set(page, { mic: "allowed" });
    await waitText(page, '[data-perm="accessibility"] .dict-perm-state', "macOS is asking you");
    assertEqual(await text(page, "#dict-state"), "Waiting for you to allow Accessibility…");
    assertEqual(await text(page, '[data-perm="microphone"] .dict-perm-state'), "Allowed");
    await page.waitForFunction(() => window.__RICHOS_MOCK__.dictationCalls().some(c => c.cmd === "dictation_ask_accessibility"));
    const order = await calls(page);
    assert(order.indexOf("dictation_ask_accessibility") > order.indexOf("dictation_ask_microphone"), "Accessibility asked after the microphone: " + order);
    // Allowed in System Settings: read within a second, the tool told, RichOS to the front.
    await set(page, { ax: "allowed", keyTap: true });
    await waitText(page, "#dict-state", "On. Tap F1 in any app, talk and tap it again.");
    assertEqual(await text(page, "#dict-feedback"), "RichScribe is on. Try it in the box on the right or in any app.");
    assert((await calls(page)).includes("dictation_permissions_changed:true"), "the tool told, RichOS brought forward");
    await page.waitForFunction(() => document.activeElement && document.activeElement.id === "dict-try");
    assertEqual(await page.getAttribute("#dict-try", "data-ph"), "Click here, tap F1 and say something.");
    assertEqual(await text(page, "#dict-try-note"), "This box is only for trying. Your words go wherever your cursor is.");
    // DICTATION IS FOR OTHER APPS (the CEO, 2026-10-13, onboarding4.png): inside RichOS, Talk to
    // Rich is the way to speak, so nothing under the message box offers dictation, on or off.
    assertEqual(await page.locator("#dictation-composer-note").count(), 0, "no dictation line under the message box");
    await page.close();
    return "microphone, then Accessibility, then On with the cursor in Try it here";
  });

  // ---- 4b. the line, when the tool reports in after the flow (D18) ---------------------------
  // The tool makes its key tap on its own time, after the window has told it the permission
  // changed (0.09 s after the switch in System Settings, walk-2882d6930ecc) or after it was
  // started by the switch (run T3: "Starting" first, On 8 s later). Check 4 above gives the tap
  // in the same breath as the grant, which is not how the app sees it; here the tap comes last.
  await run.check("D18: round 19's 'Dictation is on' line shows once the tool holds the key, when the tap comes after Accessibility was allowed and when it comes after the switch with both grants already there", async () => {
    const LINE = "RichScribe is on. Try it in the box on the right or in any app.";
    const ON = "On. Tap F1 in any app, talk and tap it again.";
    // A2's shape: the microphone, then Accessibility allowed in System Settings, then the tap.
    let page = await open("dark", {}, { tapNever: true });
    await sheet(page);
    await page.click("#dict-switch");
    await waitText(page, '[data-perm="microphone"] .dict-perm-state', "macOS is asking you");
    await set(page, { mic: "allowed" });
    // The prompt first, then the grant (as System Settings gives it): the flow reads the
    // microphone once a second, so a grant given before it has asked skips the prompt.
    await page.waitForFunction(() => window.__RICHOS_MOCK__.dictationCalls().some(c => c.cmd === "dictation_ask_accessibility"));
    await set(page, { ax: "allowed" });
    await page.waitForFunction(() => window.__RICHOS_MOCK__.dictationCalls().some(c => c.cmd === "dictation_permissions_changed" && c.forward === true));
    assertEqual(await page.locator("#dict-feedback").count(), 0, "no line before the tool holds the key");
    await set(page, { keyTap: true }); // the tool's tap, reported over rich://dictation
    await waitText(page, "#dict-state", ON);
    assertEqual(await page.locator("#dict-feedback").count(), 1, "the line, in the same paint as On");
    assertEqual(await text(page, "#dict-feedback"), LINE);
    await page.close();
    // T3's shape: both grants already there, the switch pressed; the tool starts (owner none,
    // "Starting RichScribe…"), then connects with its tap.
    page = await open("dark", { mic: "allowed", ax: "allowed" }, { owner: "none" });
    await sheet(page);
    await page.click("#dict-switch");
    await waitText(page, "#dict-state", "Starting RichScribe…");
    assertEqual(await page.locator("#dict-feedback").count(), 0, "no line while the tool starts");
    await set(page, { owner: "self", keyTap: true });
    await waitText(page, "#dict-state", ON);
    assertEqual(await page.locator("#dict-feedback").count(), 1, "the line once the tool holds the key");
    assertEqual(await text(page, "#dict-feedback"), LINE);
    // Closing the sheet clears it (round 19 `closeSheet`): opened again, the card says On alone.
    await page.keyboard.press("Escape");
    await page.waitForFunction(() => document.getElementById("dictation-sheet").hidden);
    await sheet(page);
    await waitText(page, "#dict-state", ON);
    assertEqual(await page.locator("#dict-feedback").count(), 0, "the line is the turn-on's, not the sheet's");
    await page.close();
    return "the line after the grant's tap, after the switch's tap, and gone once the sheet was closed";
  });

  await run.check("turning it off: the tool is told, the sheet says Off", async () => {
    const page = await open("dark", ALLOWED);
    await sheet(page);
    await page.click("#dict-switch");
    await waitText(page, "#dict-state", "Off. Turn it on to type with your voice in Mail, Slack, your browser, anywhere.");
    assertEqual(await calls(page), ["dictation_set_on:false"]);
    await page.close();
    return "off";
  });

  // ---- 5. the refusals ------------------------------------------------------------------------
  // Second review rv-20261013T183104Z, finding 1: Accessibility refused, the sheet closed, then
  // allowed in System Settings. Coming back to RichOS finishes setting it up: the "Dictation
  // feature" conversation is asked for without the sheet being opened again.
  await run.check("set up with the sheet closed: allowed later in System Settings, the Dictation feature conversation still comes", async () => {
    const page = await open("dark", { mic: "allowed" }, { axAsked: true });
    await sheet(page);
    await page.click("#dict-switch");
    await page.waitForFunction(() => window.__RICHOS_MOCK__.dictationCalls().some(c => c.cmd === "dictation_ask_accessibility"));
    await page.click("#dict-close");
    await page.waitForFunction(() => document.getElementById("dictation-sheet").hidden);
    assert(
      !(await page.evaluate(() => window.__RICHOS_MOCK__.dictationCalls().some(c => c.cmd === "dictation_feature_thread"))),
      "not set up yet, so no conversation"
    );
    await set(page, { ax: "allowed", keyTap: true });
    await page.evaluate(() => window.dispatchEvent(new Event("focus")));
    await page.waitForFunction(() => window.__RICHOS_MOCK__.dictationCalls().some(c => c.cmd === "dictation_feature_thread"));
    await page.close();
    return "refused, closed, allowed in System Settings: back in RichOS, dictation_feature_thread was called";
  });

  // Second review rv-20261013T193550Z, finding 1: the same, with a relaunch in the middle (macOS
  // often has him quit and reopen after allowing Accessibility). Dictation was left on, so the
  // window that starts finds it on and not yet set up; once it works, the "Dictation feature"
  // conversation is asked for, once.
  await run.check("set up across a relaunch: on with Accessibility refused, allowed while RichOS was closed, the Dictation feature conversation comes once", async () => {
    const page = await open("dark", { on: true, mic: "allowed", ax: "denied", owner: "self" }, { axAsked: true });
    const creates = () => page.evaluate(() => window.__RICHOS_MOCK__.dictationCalls().filter(c => c.cmd === "dictation_feature_thread" && c.create).length);
    await page.waitForFunction(() => window.__RICHOS_MOCK__.dictationCalls().some(c => c.cmd === "dictation_status"));
    assertEqual(await creates(), 0, "not working yet, so no conversation");
    await set(page, { ax: "allowed", keyTap: true });
    await page.evaluate(() => window.dispatchEvent(new Event("focus")));
    await page.waitForFunction(() => window.__RICHOS_MOCK__.dictationCalls().some(c => c.cmd === "dictation_feature_thread" && c.create));
    await page.locator(".nav-thread", { hasText: "RichScribe feature" }).waitFor();
    await page.evaluate(() => window.dispatchEvent(new Event("focus")));
    await menu(page);
    await page.click("#set-dictation-open");
    await page.waitForSelector("#dictation-sheet:not([hidden]) #dict-state");
    await page.click("#dict-close");
    await page.waitForFunction(() => document.getElementById("dictation-sheet").hidden);
    assertEqual(await creates(), 1, "asked for once");
    assertEqual(await page.locator(".nav-thread", { hasText: "RichScribe feature" }).count(), 1, "one conversation");
    await page.close();
    return "relaunched on and refused, allowed, back in RichOS: one Dictation feature conversation in the rail";
  });

  // Second review rv-20261013T204926Z: dictation set up before his first company. The app
  // refuses a conversation under no company, so it is made once he adds one.
  await run.check("set up before the first company: the Dictation feature conversation comes once he adds one", async () => {
    const page = await open("dark", ALLOWED, { preset: { chosenEntity: null } });
    await page.waitForFunction(() => window.__RICHOS_MOCK__.dictationCalls().some(c => c.cmd === "dictation_feature_thread" && c.create));
    assertEqual(await page.locator(".nav-thread", { hasText: "RichScribe feature" }).count(), 0, "no company, so no conversation yet");
    if (await page.locator("#memory-setup:not([hidden])").count()) await page.click("#memory-setup-later");
    await page.waitForSelector("#entity-picker:not([hidden])");
    await page.evaluate(() => [...document.querySelectorAll("#entity-picker button")].find(b => /add/i.test(b.textContent)).click());
    await page.fill("#entity-add-name", "Brightwave LABS");
    await page.click("#entity-add-go");
    await page.locator(".nav-thread", { hasText: "RichScribe feature" }).waitFor();
    assertEqual(await page.locator(".nav-thread", { hasText: "RichScribe feature" }).count(), 1, "one conversation");
    await page.close();
    return "refused under no company, then made when Brightwave LABS was added";
  });

  // Ray on the combined build (esc-20261013T210016Z-20625b89): the "Dictation feature"
  // conversation opened under the business-questions card. The card belongs to onboarding, so
  // it stays out of that conversation, and is there again back in the one he came from.
  await run.check("the business-questions card stays out of the Dictation feature conversation", async () => {
    const page = await open("dark", ALLOWED, { preset: { onboarding: "not-yet" } });
    await leaveHome(page);
    await page.waitForSelector("#first-run:not([hidden])");
    const home = await page.locator(".nav-thread-row.is-active .nav-thread").getAttribute("data-thread-id");
    const feature = page.locator(".nav-thread", { hasText: "RichScribe feature" });
    await feature.waitFor();
    await feature.click();
    await page.waitForFunction(() => document.getElementById("first-run").hidden);
    await page.click(`.nav-thread[data-thread-id="${home}"]`);
    await page.waitForSelector("#first-run:not([hidden])");
    await page.close();
    return "card in the conversation he was in, none in Dictation feature, back again on return";
  });

  await run.check("state 12: a refusal is said plainly, with Open System Settings on the right pane; back without allowing, it is said again", async () => {
    let page = await open("dark", { on: true, mic: "allowed", ax: "denied", owner: "self" }, { axAsked: true });
    await sheet(page);
    await waitText(page, "#dict-state", "Not working yet: macOS has not let me type into other apps.");
    assertEqual(await page.locator(".dict-card.is-warn").count(), 1, "the warning spine");
    assertEqual(await text(page, '[data-perm="accessibility"] .dict-perm-state'), "Not allowed Open System Settings");
    await page.click('[data-act="open-sys"][data-pane="accessibility"]');
    await waitText(page, "#dict-state", "Waiting for you to allow Accessibility…");
    assert((await calls(page)).includes("dictation_open_settings:accessibility"), "the Accessibility pane");
    // System Settings took the front and he came back without allowing.
    await page.evaluate(() => { window.dispatchEvent(new Event("blur")); window.dispatchEvent(new Event("focus")); });
    await waitText(page, "#dict-state", "Not working yet: macOS has not let me type into other apps.");
    assert(!(await calls(page)).includes("dictation_ask_accessibility"), "asked once: no second Accessibility prompt");
    await page.close();
    page = await open("dark", { on: true, mic: "denied", ax: "unknown", owner: "self" });
    await sheet(page);
    await waitText(page, "#dict-state", "Not working yet: macOS has not allowed the microphone.");
    await page.click('[data-act="open-sys"][data-pane="microphone"]');
    assert((await calls(page)).includes("dictation_open_settings:microphone"), "the Microphone pane");
    await page.close();
    // The Accessibility prompt's Deny: RichOS has the front again and is still not allowed.
    page = await open("dark", { mic: "allowed" });
    await sheet(page);
    await page.click("#dict-switch");
    await waitText(page, "#dict-state", "Waiting for you to allow Accessibility…");
    await page.evaluate(() => { window.dispatchEvent(new Event("blur")); window.dispatchEvent(new Event("focus")); });
    await waitText(page, "#dict-state", "Not working yet: macOS has not let me type into other apps.");
    await page.close();
    return "Accessibility and the microphone refused, each with its pane";
  });

  // ---- 6. the key ------------------------------------------------------------------------------
  await run.check("state 11: a key from the strip, or Press a different key with its two refusals, through the tool's tap or the window's keys", async () => {
    const page = await open("dark", ALLOWED);
    await sheet(page);
    await page.click('[data-act="key"][data-k="5"]');
    await page.waitForFunction(() => document.querySelector('[data-act="key"][data-k="5"]').getAttribute("aria-checked") === "true");
    assertEqual(await text(page, ".dict-key-row .dict-key-say"), "F5 starts and stops RichScribe. Pick one you rarely use.");
    await page.waitForFunction(() => { const t = document.getElementById("bug-toast"); return t && !t.hidden && t.textContent === "RichScribe now starts and stops with F5."; });
    await waitText(page, "#dict-state", "On. Tap F5 in any app, talk and tap it again.");
    await page.click("#dict-capture");
    await waitText(page, ".dict-key-wait .dict-key-say", "Press the key you want to use. Esc to cancel.");
    assertEqual(await text(page, ".dict-sec .dict-key-say.is-soft"), "Any function key works, F1 to F19.");
    await page.keyboard.press("Shift");
    await waitText(page, "#dict-key-err", "Command, Option, Control and Shift can't be used: anything you type while one is down would set off shortcuts. Use a function key, F1 to F19.");
    await page.keyboard.press("a");
    await waitText(page, "#dict-key-err", "That key types text. Use a function key, F1 to F19.");
    // The window's own keys: F9.
    await page.keyboard.press("F9");
    await page.waitForFunction(() => document.querySelector('[data-act="key"][data-k="9"]') && document.querySelector('[data-act="key"][data-k="9"]').getAttribute("aria-checked") === "true");
    assertEqual(await page.locator(".dict-keys > *").allInnerTexts(), ["F6", "F7", "F8", "F9", "F10", "F11", "F12"], "the strip follows a key past F7");
    // Through the tool's tap: an Apple top-row key captured as the key it is (F11).
    await page.click("#dict-capture");
    await page.waitForSelector(".dict-key-wait");
    await page.evaluate(() => window.__RICHOS_MOCK__.dictationKey(11));
    await page.waitForFunction(() => document.querySelector('[data-act="key"][data-k="11"]') && document.querySelector('[data-act="key"][data-k="11"]').getAttribute("aria-checked") === "true");
    // Escape during capture cancels the capture only.
    await page.click("#dict-capture");
    await page.waitForSelector(".dict-key-wait");
    await page.keyboard.press("Escape");
    await page.waitForSelector(".dict-key-row");
    assert(await page.locator("#dictation-sheet").isVisible(), "the sheet stays open");
    assertEqual((await calls(page)).filter(c => c.startsWith("dictation_set_key") || c.startsWith("dictation_capture_key")),
      ["dictation_set_key:5", "dictation_capture_key:true", "dictation_capture_key:false", "dictation_set_key:9", "dictation_capture_key:true", "dictation_capture_key:false", "dictation_set_key:11", "dictation_capture_key:true", "dictation_capture_key:false"]);
    await page.close();
    return "F5 from the strip, F9 by the window's keys, F11 by the tap; Shift and A refused";
  });

  // ---- 7. accuracy --------------------------------------------------------------------------
  await run.check("Accuracy: Faster is chosen and saved, and says so", async () => {
    const page = await open("dark", ALLOWED);
    await sheet(page);
    await page.click('[data-act="accuracy"][data-v="fast"]');
    await waitText(page, ".dict-saved", "Saved.");
    assertEqual(await page.getAttribute('[data-act="accuracy"][data-v="fast"]', "aria-checked"), "true");
    assertEqual(await page.getAttribute('[data-act="accuracy"][data-v="accurate"]', "aria-checked"), "false");
    assert((await calls(page)).includes("dictation_set_accuracy:fast"));
    await page.close();
    return "Faster, Saved.";
  });

  // ---- 8. the more lines ------------------------------------------------------------------
  await run.check("the second copy and line 4 on the sheet: On through another copy's tool (round 19's On line, never 'even when closed'); another app hiding keys, named and unnamed", async () => {
    let page = await open("dark", { ...ALLOWED, owner: "other" });
    await sheet(page);
    await waitText(page, "#dict-state", "On. Tap F1 in any app, talk and tap it again.");
    assertEqual(await page.locator(".dict-card.is-on").count(), 1, "the gold on card: dictation does work");
    await page.close();
    // The other copy's tool types, so this copy's own permissions decide nothing (plan section 7,
    // "Two copies with dictation on"): seen in walk-7350bc140616, where a second copy without
    // its own grants said "Needs a permission". It says round 19's On line, with line 2 (every
    // copy works only while RichOS is open), and asks macOS for nothing.
    page = await open("light", { on: true, owner: "other", keyTap: true, mic: "denied", ax: "unknown" });
    await menu(page);
    await waitText(page, "#set-dictation-state", "On. Tap F1 to talk");
    assertEqual(await page.locator("#set-dictation-state.is-attention").count(), 0, "row: no attention");
    await page.click("#set-dictation-open");
    await waitText(page, "#dict-state", "On. Tap F1 in any app, talk and tap it again.");
    assertEqual(await page.locator("#dict-copy-note").count(), 1, "line 2 for the second copy too");
    assertEqual(await page.locator(".dict-card.is-warn").count(), 0, "no warning spine");
    const asked = (await calls(page)).filter(c => /ask_(microphone|accessibility)/.test(c));
    assertEqual(asked.join(","), "", "macOS is asked for nothing");
    await page.close();
    // Never On without a working tool (recheck finding 6): no tool yet, and a tool with no tap.
    page = await open("dark", { ...ALLOWED, owner: "none", keyTap: false });
    await sheet(page);
    await waitText(page, "#dict-state", "Starting RichScribe…");
    assertEqual(await page.locator(".dict-card.is-on").count(), 0, "not the gold on card");
    assertEqual(await page.getAttribute("#dict-try", "data-ph"), "You can try it here once RichScribe has started.");
    await page.close();
    // The sheet pokes the tool once for a tap (dictation_permissions_changed); here the tool's
    // tap never comes, which is the case this line is for.
    page = await open("dark", { ...ALLOWED, keyTap: false }, { tapNever: true });
    await sheet(page);
    await waitText(page, "#dict-state", "Not working yet: the key is not being watched. Turn the switch off and on again.");
    assertEqual(await page.locator(".dict-card.is-warn").count(), 1, "the warning spine");
    assertEqual(await page.locator("#dict-feedback").count(), 0, "no 'Dictation is on' feedback");
    await page.close();
    page = await open("dark", { ...ALLOWED, secure: { app: "1Password" } });
    await sheet(page);
    await waitText(page, "#dict-state", "Paused: 1Password is hiding your keys from other apps, so F1 can't reach me. Quitting 1Password fixes it.");
    assertEqual(await page.locator(".dict-card.is-warn").count(), 1, "the warning spine");
    assertEqual(await page.getAttribute("#dict-try", "data-ph"), "You can try it here once your keys are back.");
    assert(await page.locator("#dictation-composer-note").count() === 0 || await page.locator("#dictation-composer-note").isHidden(), "the composer drops its line");
    await set(page, { secure: null });
    await waitText(page, "#dict-state", "On. Tap F1 in any app, talk and tap it again.");
    await page.close();
    page = await open("dark", { ...ALLOWED, secure: { app: null } });
    await sheet(page);
    await waitText(page, "#dict-state", "Paused: Another app is hiding your keys, so F1 can't reach me. Quitting the app where you last typed a password usually fixes it.");
    await page.close();
    return "line 1, line 4 named and unnamed, and back to On";
  });

  // ---- 8c. a sheet request made before the page existed (third review, finding 4) --------------
  await run.check("a Dictation settings or Fix it request made while no page existed opens the sheet as the page boots, and is taken once", async () => {
    const page = await open("dark", ALLOWED, { sheetPending: true });
    await page.waitForSelector("#dict-switch");
    const taken = await page.evaluate(() => window.__RICHOS_MOCK__.dictationCalls().filter(c => c.cmd === "dictation_take_sheet_request").length);
    assert(taken >= 1, "the page took the request as it booted");
    await page.close();
    const plain = await open("dark", ALLOWED);
    await plain.waitForSelector("#set-btn");
    assertEqual(await plain.locator("#dict-switch").count(), 0, "no request, no sheet");
    await plain.close();
    return "the sheet opened from the kept request, and a page with none opened nothing";
  });

  // ---- 9. the off notice ---------------------------------------------------------------------
  await run.check("the off notice: the key pressed in RichOS's window while dictation is off says where to turn it on", async () => {
    const page = await open("dark", {});
    await page.waitForFunction(() => window.__RICHOS_MOCK__.dictationCalls().length > 0);
    await page.keyboard.press("F1");
    await page.waitForFunction(() => { const t = document.getElementById("bug-toast"); return t && !t.hidden && t.textContent === "RichScribe is off. Turn it on in Settings, under RichScribe."; });
    await page.close();
    const on = await open("dark", ALLOWED);
    await on.waitForFunction(() => window.__RICHOS_MOCK__.dictationCalls().length > 0);
    await on.keyboard.press("F1");
    assertEqual(await on.locator("#bug-toast:not([hidden])").count(), 0, "with dictation on, no notice");
    await on.close();
    return "said while off, not while on";
  });

  // ---- 10. Rich's offer ---------------------------------------------------------------------
  await run.check("Rich's offer: window.RichDictation.turnOn opens the sheet, runs the same flow, and answers {on} once it settles", async () => {
    let page = await open("dark", { mic: "allowed", ax: "allowed" });
    await page.waitForFunction(() => window.__RICHOS_MOCK__.dictationCalls().length > 0);
    const yes = await page.evaluate(() => window.RichDictation.turnOn());
    assertEqual(yes, { on: true });
    assert(await page.locator("#dictation-sheet").isVisible(), "the sheet is open on it");
    await page.close();
    page = await open("dark", { mic: "denied" });
    await page.waitForFunction(() => window.__RICHOS_MOCK__.dictationCalls().length > 0);
    const no = await page.evaluate(() => window.RichDictation.turnOn());
    assertEqual(no, { on: false });
    await page.close();
    return "on when both are allowed, not on when the microphone is refused";
  });

  // ---- 11. Escape -----------------------------------------------------------------------------
  await run.check("Escape closes the sheet", async () => {
    const page = await open("dark", {});
    await sheet(page);
    await page.keyboard.press("Escape");
    await page.waitForFunction(() => document.getElementById("dictation-sheet").hidden);
    await page.close();
    return "closed";
  });

  // ---- every state, both themes ------------------------------------------------------------------
  const STATES = [
    ["row-off", {}, menu],
    ["row-on", ALLOWED, menu],
    ["row-needs", { on: true, mic: "denied" }, menu],
    ["row-waiting", { on: true, mic: "asking" }, menu],
    ["row-other", { ...ALLOWED, owner: "other" }, menu],
    ["row-paused", { ...ALLOWED, secure: { app: "1Password" } }, menu],
    ["sheet-off", {}, sheet],
    ["ask-mic", { on: true, mic: "asking", owner: "self" }, sheet],
    ["ask-ax", { mic: "allowed" }, async p => { await sheet(p); await p.click("#dict-switch"); await p.waitForSelector(".dict-ring"); }],
    ["sheet-on", { mic: "allowed", ax: "allowed" }, async p => { await sheet(p); await p.click("#dict-switch"); await p.waitForSelector("#dict-feedback"); }],
    ["key-pick", ALLOWED, async p => { await sheet(p); await p.click("#dict-capture"); await p.keyboard.press("Shift"); await p.waitForSelector("#dict-key-err"); }],
    ["key-f9", { ...ALLOWED, key: 9 }, sheet],
    ["denied", { on: true, mic: "allowed", ax: "denied", owner: "self" }, sheet],
    ["mic-denied", { on: true, mic: "denied", owner: "self" }, sheet],
    ["saved", ALLOWED, async p => { await sheet(p); await p.click('[data-act="accuracy"][data-v="fast"]'); await p.waitForSelector(".dict-saved"); }],
    ["other", { ...ALLOWED, owner: "other" }, sheet],
    ["secure", { ...ALLOWED, secure: { app: "1Password" } }, sheet],
    ["secure-anon", { ...ALLOWED, secure: { app: null } }, sheet],
    ["starting", { ...ALLOWED, owner: "none", keyTap: false }, sheet],
    ["no-tap", { ...ALLOWED, keyTap: false }, sheet],
    ["row-no-tap", { ...ALLOWED, keyTap: false }, menu],
  ];
  const ROOTS = "#dictation-sheet:not([hidden]) .dict-panel, #set-dictation-open";
  // Declared skippable (style.css's own note): the 14px "Settings" breadcrumb over the 32px title.
  const SKIPPABLE = ".dict-eyebrow";
  for (const theme of ["dark", "light"]) {
    await run.check(theme + ": every slice 2 state meets AA contrast and the type floor, fits, and carries no m-dash or n-dash", async () => {
      const measured = [];
      for (const [name, fixture, drive] of STATES) {
        for (const viewport of theme === "dark" ? [{ width: 1440, height: 900 }, { width: 1024, height: 700 }] : [{ width: 1440, height: 900 }]) {
          const page = await open(theme, fixture, { viewport, axAsked: fixture.ax === "denied" });
          await drive(page);
          await page.evaluate(() => Promise.all(document.getAnimations().filter(a => a.effect && a.effect.getTiming().iterations !== Infinity).map(a => a.finished.catch(() => null))));
          await page.addScriptTag({ content: contrast.pageScript() });
          const result = await page.evaluate(({ ROOTS, SKIPPABLE }) => {
            const C = window.__contrastMath, failures = [], dashes = [];
            let worst = 99, worstIndicator = 99, nodes = 0;
            const ground = el => { let bg = { r: 0, g: 0, b: 0, a: 0 }; const chain = []; for (let p = el; p; p = p.parentElement) chain.unshift(p);
              for (const p of chain) bg = C.compositeOver(C.parseCssColor(getComputedStyle(p).backgroundColor), bg); return bg; };
            // A color the parser cannot read is unprovable, which is a failure, never a pass.
            const ratioOn = (paint, bg) => { const c = C.parseCssColor(paint); return c ? C.round2(C.contrastRatio(C.compositeOver(c, bg), bg)) : 0; };
            for (const root of document.querySelectorAll(ROOTS)) {
              if (!root.getClientRects().length) continue;
              if (/[–—]/.test(root.innerText)) dashes.push(root.innerText.match(/.{0,30}[–—].{0,30}/)[0]);
              for (const e of [root, ...root.querySelectorAll("*")]) {
                if (!e.getClientRects().length || e.closest("[hidden]")) continue;
                const own = [...e.childNodes].some(n => n.nodeType === Node.TEXT_NODE && n.textContent.trim());
                if (!own) continue;
                const style = getComputedStyle(e), bg = ground(e), fg = C.compositeOver(C.parseCssColor(style.color), bg);
                const size = parseFloat(style.fontSize), large = size >= 24 || size >= 18.66 && parseInt(style.fontWeight) >= 700;
                const ratio = C.round2(C.contrastRatio(fg, bg)); nodes++;
                worst = Math.min(worst, ratio);
                if (ratio < (large ? 3 : 4.5)) failures.push({ text: e.textContent.trim().slice(0, 50), ratio });
                if (size < 16 && !e.closest(SKIPPABLE)) failures.push({ text: e.textContent.trim().slice(0, 50), size });
              }
              // The placeholder in Try it here is text he reads: its color on the box's ground.
              const box = root.querySelector && root.querySelector("#dict-try:empty");
              if (box && box.getClientRects().length) {
                const ph = getComputedStyle(box, "::before"), bg = ground(box);
                const ratio = C.round2(C.contrastRatio(C.compositeOver(C.parseCssColor(ph.color), bg), bg));
                worst = Math.min(worst, ratio);
                if (ratio < 4.5) failures.push({ text: "placeholder: " + box.dataset.ph, ratio });
              }
              // Non-text indicators, 3:1 against the ground they sit on. A control that both fills
              // and draws a border is judged by the stronger of the two (the boundary the eye
              // sees, round 19 NOTES); a spinner by its moving gold arc.
              for (const sel of [".dict-switch", ".dict-key:not(.is-esc)", ".dict-card.is-on .dict-spine", ".dict-card.is-warn .dict-spine", ".dict-radio", ".dict-btn:not(.dict-btn-quiet)", ".dict-ring", ".dict-orb", ".dict-meter i", "#dict-close", ".dict-try"]) {
                for (const e of root.querySelectorAll(sel)) {
                  if (!e.getClientRects().length || e.closest("[hidden]")) continue;
                  const cs = getComputedStyle(e), bg = ground(e.parentElement);
                  let paints = [cs.backgroundColor, cs.borderTopColor];
                  if (e.matches(".dict-card.is-warn .dict-spine")) paints = [cs.color];
                  if (e.matches(".dict-ring")) paints = [cs.borderTopColor];
                  const ratio = Math.max(...paints.filter(p => p && p !== "rgba(0, 0, 0, 0)").map(p => ratioOn(p, bg)), 0);
                  worstIndicator = Math.min(worstIndicator, ratio);
                  if (ratio < 3) failures.push({ indicator: sel, ratio, paints });
                }
              }
            }
            const body = document.querySelector("#dictation-sheet:not([hidden]) .dict-body");
            const sideways = body ? body.scrollWidth > body.clientWidth + 1 : false;
            return { failures, dashes, worst, worstIndicator, nodes, sideways };
          }, { ROOTS, SKIPPABLE });
          assertEqual(result.failures, [], `${theme} ${name} ${viewport.width}: contrast and type`);
          assertEqual(result.dashes, [], `${theme} ${name}: no m-dash or n-dash`);
          assert(!result.sideways, `${theme} ${name} ${viewport.width}: never scrolls sideways`);
          assert(result.nodes > 0, `${theme} ${name}: EMPTY INVENTORY`);
          if (viewport.width === 1440) measured.push(`${name} ${result.worst}:1${result.worstIndicator < 99 ? "/" + result.worstIndicator + ":1" : ""}`);
          // Every control is reachable, by scrolling where the window is small.
          const controls = page.locator("#dictation-sheet:not([hidden]) button, #dictation-sheet:not([hidden]) [contenteditable]");
          for (let i = 0, n = await controls.count(); i < n; i++) {
            const control = controls.nth(i);
            await control.scrollIntoViewIfNeeded(); assert(await control.isVisible(), `${theme} ${name} ${viewport.width}: control ${i} is out of reach`);
          }
          await page.close();
        }
      }
      return "worst text/indicator per state: " + measured.join(", ");
    });
  }

  await run.check("no renderer errors", async () => assertEqual(errors, []));
  await browser.close();
  process.exitCode = run.report() ? 1 : 0;
}
main().catch(e => { console.error(e); process.exitCode = 1; });

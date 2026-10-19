"use strict";
// CLAUDE ACCOUNTS, FOR SOMEONE WHO IS NOT TECHNICAL: round 18 (richos-hq
// design/mockups/rounds/round-18/multi-account.html, NOTES.md), measured against the CEO's
// words, feedback item 9 of 2026-10-06: "Non-technical users should also be able to take
// advantage of the multi-account setup". One check per part of the round, each red on main
// (where there is no row, no sheet and no buttons under Rich's lines), then every state of the
// round in both themes for contrast, type size and fit, computed in WebKit.
const path = require("path");
const contrast = require("./lib/contrast");
const { loadPlaywright, createRun, assert, assertEqual, UI_DIR, leaveHome } = require("./lib/harness");

const HOUR = 3600000, DAY = 24 * HOUR;
const now = Date.now();
const win = (five, week, fiveAt = now + 2 * HOUR, weekAt = now + 2 * DAY) => [
  { id: "five_hour", label: "Five-hour", usedPercent: five, resetsAt: fiveAt, durationMs: 5 * HOUR },
  { id: "seven_day", label: "Weekly", usedPercent: week, resetsAt: weekAt, durationMs: 7 * DAY },
];
const one = (week, five = 22) => ({ state: "fresh", checkedAt: now, retryAt: null, message: null, windows: win(five, week) });
function two(home, work, opts = {}) {
  const working = opts.working || {};
  const homeAcct = { id: "1", label: "Home", inUse: opts.inUse !== "2", working: working["1"] || [], windows: win(home[0], home[1], now + 2 * HOUR, now + 2 * DAY), checkedAt: now, exhaustedUntil: opts.homeGone || null, message: null };
  const workAcct = { id: "2", label: "Work", inUse: opts.inUse === "2", working: working["2"] || [], windows: win(work[0], work[1], now + 3 * HOUR, now + 3 * DAY), checkedAt: now, exhaustedUntil: null, message: null };
  const used = opts.inUse === "2" ? workAcct : homeAcct;
  return { state: "fresh", checkedAt: now, retryAt: null, message: null, windows: used.windows, accounts: [homeAcct, workAcct],
    changes: opts.changes || [{ at: now - DAY - 2 * HOUR, kind: "added", id: "2", label: "Work" }], lastSwitch: opts.lastSwitch || null };
}

async function main() {
  const run = createRun("Claude accounts for everyone (round 18)");
  const browser = await loadPlaywright().webkit.launch();
  const errors = [];
  async function open(theme, quota, opts = {}) {
    const page = await browser.newPage({ viewport: opts.viewport || { width: 1440, height: 900 }, locale: "en-US" });
    page.setDefaultTimeout(30000); // load-bound: a hang guard only; every wait below waits for a fact
    page.on("pageerror", e => errors.push(String(e)));
    await page.addInitScript(({ theme, quota, signIn, policy }) => {
      localStorage.setItem("richos-theme", theme);
      localStorage.setItem("richos-mock-config", JSON.stringify({ theme, font_scale: 100 }));
      localStorage.setItem("richos-mock-quota-policy", JSON.stringify(policy || { enabled: false, pausePercent: 93 }));
      window.__RICHOS_MOCK_PRESET__ = { quota, signIn };
    }, { theme, quota, signIn: opts.signIn || [], policy: opts.policy });
    await page.goto("file://" + path.join(UI_DIR, "index.html"));
    await page.waitForSelector("#set-btn");
    await page.evaluate(() => window.RichSplash.yieldNow("accounts-test"));
    await page.evaluate(t => document.documentElement.setAttribute("data-theme", t), theme);
    return page;
  }
  const menu = async page => { await page.click("#set-btn"); await page.waitForSelector("#set-accounts-open"); };
  async function sheet(page) {
    await menu(page);
    await page.waitForFunction(() => document.getElementById("set-accounts-state").textContent.length > 0);
    await page.click("#set-accounts-open");
    await page.waitForSelector("#accounts-sheet:not([hidden]) .acc-card");
  }
  // On a conversation, so Rich's lines have somewhere to be said.
  async function onThread(page) {
    await leaveHome(page);
    await page.waitForFunction(() => window.__RICHOS_TIMELINE__ && window.__RICHOS_TIMELINE__().threadId);
  }
  const say = (page, kind, line) => page.evaluate(([k, l]) => window.__RICHOS_MOCK__.simulateAccountNote(k, l, window.__RICHOS_TIMELINE__().threadId), [kind, line]);
  const text = (page, sel) => page.locator(sel).innerText().then(t => t.replace(/\s+/g, " ").trim());
  const texts = (page, sel) => page.locator(sel).allInnerTexts().then(a => a.map(t => t.replace(/\s+/g, " ").trim()));
  const clock = t => new Date(Math.round(t / 60000) * 60000).toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit" });

  // ---- 1. the row, for everyone ----------------------------------------------------------
  await run.check("the row: Claude accounts is in Settings with Technical view off, in the place of the old account row, with its plain line", async () => {
    let page = await open("dark", one(41));
    await menu(page);
    assert(await page.locator("#set-accounts-open").isVisible(), "the row is there with Technical view off");
    assert(!(await page.locator("#set-techy").isChecked()), "Technical view is off");
    assert(await page.locator("#set-quota-open").isHidden(), "the detailed quota row stays technical");
    assertEqual(await page.locator("#set-account-open").count(), 0, "the old account row is gone");
    // One account with room in its week: no second line (it would push Bust a bug out of the
    // smallest window, tests/settings-fit.js), and the week's bar beside the name.
    await page.waitForFunction(() => document.querySelector("#set-accounts-open .acc-mini b, #set-accounts-open .acc-mini i"));
    assert(await page.locator("#set-accounts-state").isHidden(), "no second line with one account and room");
    assert(await page.locator("#set-accounts-open .acc-mini").isVisible(), "the week's mini bar");
    const order = await page.locator("#set-menu button.bugbtn").evaluateAll(b => b.map(x => x.id));
    assert(order.indexOf("set-accounts-open") < order.indexOf("set-repositories-open"), "above the folders row, as drawn: " + order);
    await page.close();
    page = await open("dark", one(86));
    await menu(page);
    await page.waitForFunction(() => document.getElementById("set-accounts-state").textContent === "86% of this week used");
    assert(await page.locator("#set-accounts-state.is-attention").isVisible(), "near the week's end it is said in gold");
    await page.close();
    page = await open("light", two([20, 72], [0, 12]));
    await menu(page);
    await page.waitForFunction(() => document.getElementById("set-accounts-state").textContent === "Using Home");
    await page.close();
    const at = now - 10 * 60000;
    page = await open("light", two([64, 99], [6, 4], { inUse: "2", lastSwitch: { from: "1", to: "2", at, why: "weekly", used: 99 } }));
    await menu(page);
    await page.waitForFunction(want => document.getElementById("set-accounts-state").textContent === want, `Using Work, switched ${clock(at)}`);
    // The time never breaks before its AM or PM (the 2026-10-06 walk left "PM" alone on a line).
    assertEqual(await page.evaluate(() => {
      const t = document.querySelector("#set-accounts-state .acc-nowrap");
      return t ? [t.textContent, getComputedStyle(t).whiteSpace] : null;
    }), [clock(at), "nowrap"], "the switch time is kept on one line");
    await page.close();
    return "no line with one account / 86% of this week used / Using Home / Using Work, switched " + clock(at);
  });

  // ---- 2. the cards ------------------------------------------------------------------------
  await run.check("the cards: one account with the place a second would go; two in the order Rich uses them, with plain meters", async () => {
    let page = await open("dark", one(86, 47));
    await sheet(page);
    assertEqual(await text(page, "#acc-title"), "Claude accounts");
    assertEqual(await texts(page, ".acc-name"), ["Your Claude account"]);
    assertEqual(await texts(page, ".acc-pill"), ["In use now"]);
    assertEqual(await texts(page, '.acc-meter[data-k="week"] .acc-meter-label, .acc-meter[data-k="five"] .acc-meter-label'), ["Weekly limit", "5-hour limit"]);
    assertEqual(await text(page, '.acc-meter[data-k="week"] .acc-meter-num'), "86%used");
    assert((await text(page, '.acc-meter[data-k="week"] .acc-meter-when')).startsWith("Fresh again "), "Fresh again <when>");
    assert(await page.locator(".acc-slot").isVisible(), "the empty place a second card would go");
    assertEqual(await text(page, ".acc-how-title"), "When your account fills up");
    await page.close();
    page = await open("dark", two([58, 72], [0, 12]));
    await sheet(page);
    assertEqual(await texts(page, ".acc-name"), ["Home", "Work"]);
    assertEqual(await texts(page, ".acc-pill"), ["In use now", "Next"]);
    assertEqual(await text(page, ".acc-handover"), "Then Work, when Home is nearly full");
    assertEqual(await text(page, '.acc-card[data-id="2"] .acc-meter[data-k="five"] .acc-meter-when'), "Not used in the last 5 hours");
    assertEqual(await page.locator('.acc-card[data-id="1"] button').allInnerTexts(), [], "the one in use has nothing to press, and Account 1 is never removed here");
    assertEqual((await page.locator('.acc-card[data-id="2"] button').allInnerTexts()).map(t => t.trim()), ["Use this one now", "Remove"]);
    assertEqual(await text(page, ".acc-how-title"), "How Rich uses your accounts");
    await page.close();
    return "one: Your Claude account + the slot; two: Home In use now, Work Next, the handover line";
  });

  await run.check("Use this one now: the cards trade places, Recent changes says so, Undo puts it back; a near-full account says when Rich switches again", async () => {
    let page = await open("dark", two([58, 72], [0, 12]));
    await sheet(page);
    await page.click('[aria-label="Use Work now"]');
    await page.waitForFunction(() => document.querySelector(".acc-card").dataset.id === "2");
    assertEqual(await texts(page, ".acc-name"), ["Work", "Home"]);
    assertEqual(await text(page, ".acc-feedback"), "Rich now uses Work. Home is next. Undo");
    assertEqual((await texts(page, ".acc-log li"))[0].replace(/^\S+ \S+ /, ""), "You chose Work.");
    await page.click('.acc-feedback [data-act="undo"]');
    await page.waitForFunction(() => document.querySelector(".acc-card").dataset.id === "1");
    await page.close();
    page = await open("dark", two([58, 72], [10, 93]));
    await sheet(page);
    await page.click('[aria-label="Use Work now"]');
    await page.waitForSelector(".acc-feedback");
    assertEqual(await text(page, ".acc-feedback"), "Rich now uses Work. Work has already used 93% of its week, so Rich will switch again when it reaches 99%. Undo");
    await page.close();
    return "Work first, then Home; the 93% account says the 99% switch";
  });

  // ---- 3. adding a second account in two steps ----------------------------------------------
  await run.check("adding a second account in two steps: name both, then the browser sign-in; the new card arrives ready", async () => {
    const page = await open("dark", one(86, 47));
    await sheet(page);
    await page.click(".acc-slot");
    await page.waitForSelector("#acc-add");
    assertEqual(await text(page, ".acc-add-step"), "Step 1 of 2");
    assertEqual(await texts(page, ".acc-field"), ["The account you use now", "The new account"]);
    await page.click("#acc-continue");
    assertEqual(await text(page, ".acc-err"), "Give the new account a name, for example Work.");
    await page.fill("#acc-cur", "Home"); await page.fill("#acc-nu", "home");
    await page.click("#acc-continue");
    assertEqual(await text(page, ".acc-err"), "That name is taken. Pick a different one.");
    await page.fill("#acc-nu", "Work");
    await page.press("#acc-nu", "Enter");
    await page.waitForSelector("#acc-open-signin");
    assertEqual(await text(page, ".acc-add-title"), "Sign in to Work");
    assertEqual(await text(page, ".acc-add-step"), "Step 2 of 2");
    assert((await text(page, "#acc-add")).includes("RichOS never sees your password."));
    await page.click("#acc-open-signin");
    await page.waitForSelector(".acc-waiting");
    assertEqual(await text(page, ".acc-waiting"), "Waiting for you to finish signing in…");
    assertEqual(await page.evaluate(() => window.__accountCalls.find(c => c.cmd === "claude_account_add")), { cmd: "claude_account_add", label: "Work", currentLabel: "Home" });
    await page.waitForSelector(".acc-feedback");
    assertEqual(await text(page, ".acc-feedback"), "Work is ready. Rich switches to it when Home is nearly full.");
    assertEqual(await texts(page, ".acc-name"), ["Home", "Work"]);
    assert((await texts(page, ".acc-log li")).some(t => t.endsWith("You added Work.")), "Recent changes");
    await page.close();
    return "named Home and Work, signed in, Work ready";
  });

  await run.check("the same account signed in again gets its own plain screen and Try again; Cancel leaves nothing behind", async () => {
    let page = await open("light", one(86, 47), { signIn: ["same-account", "connected"] });
    await sheet(page);
    await page.click(".acc-slot");
    await page.fill("#acc-nu", "Work");
    await page.click("#acc-continue");
    await page.click("#acc-open-signin");
    await page.waitForSelector("#acc-try-again");
    // The account being added is not a card until its sign-in is the right one: the 2026-10-06
    // VM walk drew Work as "Next" with Use this one now while this screen said it was not added.
    assertEqual(await page.locator(".acc-card").count(), 1, "the account being added is not a card yet");
    assertEqual(await text(page, ".acc-card .acc-name"), "Home", "the one card has the name step 1 gave it");
    assertEqual(await text(page, ".acc-add-title"), "That is the account you already use");
    assert((await text(page, "#acc-add")).includes("You signed in as Home again. To add Work, sign in with your other Claude account."));
    assert((await text(page, "#acc-add")).includes("In your browser, sign out of Claude first, then press Try again."));
    await page.click("#acc-try-again");
    await page.waitForSelector(".acc-feedback");
    assertEqual(await text(page, ".acc-feedback"), "Work is ready. Rich switches to it when Home is nearly full.");
    await page.close();
    page = await open("light", one(86, 47), { signIn: ["same-account"] });
    await sheet(page);
    await page.click(".acc-slot");
    await page.fill("#acc-nu", "Work");
    await page.click("#acc-continue");
    await page.click("#acc-open-signin");
    await page.waitForSelector("#acc-try-again");
    await page.click('#acc-add [data-act="add-cancel"]');
    await page.waitForFunction(() => window.__accountCalls.some(c => c.cmd === "claude_account_discard"));
    await page.waitForSelector(".acc-slot");
    assertEqual(await page.locator(".acc-card").count(), 1, "back to one account");
    await page.close();
    return "same-account screen, Try again to ready; Cancel discards";
  });

  // ---- 4. why it switched, in three places ------------------------------------------------
  await run.check("why it switched, in three places: Rich's line with See your accounts, the banner with when and why, Recent changes", async () => {
    const page = await open("dark", two([20, 96], [6, 4]));
    await onThread(page);
    await page.evaluate(() => window.__RICHOS_MOCK__.simulateAccountSwitch("1", "2", "weekly", 99));
    await say(page, "switched", "I switched the team to your **Work** account. Home had used 99% of its weekly limit, and it is fresh again on Thursday at 9:00 AM.\n\nNothing stopped. The team carried on right where it was.");
    await page.waitForSelector('[data-note-act="see"]');
    assertEqual(await text(page, '[data-note-act="see"]'), "See your accounts");
    await page.click('[data-note-act="see"]');
    await page.waitForSelector("#accounts-sheet:not([hidden]) .acc-banner");
    const at = await page.evaluate(() => window.RichBridge.invoke("claude_quota", {}).then(v => v.lastSwitch.at));
    assertEqual(await text(page, ".acc-banner-head"), `Rich switched to Work at ${clock(at)}`);
    assert((await text(page, ".acc-banner-body")).startsWith("Why: Home had used 99% of its weekly limit. It is fresh again "), await text(page, ".acc-banner-body"));
    assert((await text(page, ".acc-banner-body")).endsWith("Nothing stopped and the team carried on."));
    assertEqual((await texts(page, ".acc-log li"))[0], `${clock(at)} Rich switched to Work. Home reached 99% of its week.`);
    assertEqual(await texts(page, ".acc-name"), ["Work", "Home"], "the one in use now is on top");
    assertEqual(await texts(page, ".acc-pill"), ["In use now", "Next"]);
    await page.click('.acc-banner [data-act="banner-ok"]');
    await page.waitForFunction(() => !document.querySelector(".acc-banner"));
    await page.close();
    return "line + See your accounts, banner, Recent changes";
  });

  // ---- 4b. a switch still under way ----------------------------------------------------------
  // Review rv-20261019T005929Z-e3d9ad03-3e7b, finding 3: the switch to Work is decided, Rich's
  // reply already ran on Work, and the team (Stu) is still on Home finishing its step. The panel
  // said Work was "In use now" and "Rich and the team are working on this one" while the team's
  // job still ran on Home. Each account says who is really on it until the move is done.
  await run.check("a switch under way: the account being left says who is still on it, the new one says Switching to this one", async () => {
    let page = await open("dark", two([64, 99], [6, 4], { inUse: "2", working: { "1": ["team"], "2": ["rich"] } }));
    await menu(page);
    await page.waitForFunction(() => document.getElementById("set-accounts-state").textContent === "Switching to Work");
    await page.click("#set-accounts-open");
    await page.waitForSelector("#accounts-sheet:not([hidden]) .acc-card");
    assertEqual(await texts(page, ".acc-name"), ["Work", "Home"], "the account being left comes right after the one in use");
    assertEqual(await texts(page, ".acc-pill"), ["Switching to this one", "Still in use"]);
    assertEqual(await text(page, '.acc-card[data-id="2"] .acc-note'), "Rich is working on this one now. The team moves here once it finishes the step it is on.");
    assertEqual(await text(page, '.acc-card[data-id="1"] .acc-note'), "The team is finishing the step it is on here, then moves to Work.");
    assertEqual(await page.locator(".acc-handover").count(), 0, "no Then line while the move is under way");
    await page.close();
    page = await open("dark", two([64, 99], [6, 4], { inUse: "2", working: { "1": ["rich"], "2": ["team"] } }));
    await sheet(page);
    assertEqual(await text(page, '.acc-card[data-id="1"] .acc-note'), "Rich moves to Work with his next reply.");
    assertEqual(await text(page, '.acc-card[data-id="2"] .acc-note'), "The team is working on this one now. Rich moves here with his next reply.");
    await page.close();
    // Once everyone is on Work, it is simply In use now.
    page = await open("dark", two([64, 99], [6, 4], { inUse: "2", working: { "2": ["rich", "team"] } }));
    await menu(page);
    await page.waitForFunction(() => document.getElementById("set-accounts-state").textContent === "Using Work");
    await page.click("#set-accounts-open");
    await page.waitForSelector("#accounts-sheet:not([hidden]) .acc-card");
    assertEqual((await texts(page, ".acc-pill"))[0], "In use now");
    await page.close();
    return "Switching to Work / Still in use, who is where; then In use now";
  });

  // ---- 5. Rich's one-time suggestion -------------------------------------------------------
  await run.check("Rich's suggestion at 86%: Add a second account and Not now under his line; Not now says where to find it", async () => {
    const page = await open("dark", one(86, 47));
    await onThread(page);
    await say(page, "nudge", "Your Claude account has used **86%** of this week's limit.");
    await page.waitForSelector('[data-note-act="add"]');
    assertEqual(await texts(page, ".acc-note-acts button"), ["Add a second account", "Not now"]);
    await page.click('[data-note-act="add"]');
    await page.waitForSelector("#accounts-sheet:not([hidden]) #acc-add");
    assertEqual(await text(page, ".acc-add-title"), "Add a second Claude account");
    await page.keyboard.press("Escape");
    await page.waitForSelector(".acc-slot");
    await page.keyboard.press("Escape");
    await page.waitForFunction(() => document.getElementById("accounts-sheet").hidden);
    await page.click('[data-note-act="not-now"]');
    await page.waitForSelector(".acc-after");
    assertEqual(await text(page, ".acc-after"), "Okay. You can add one any time in Settings, under Claude accounts.");
    await page.close();
    return "Add opens step 1; Not now answers";
  });

  await run.check("both near the weekly limit: the banner says what happens when both run out, with Add another account", async () => {
    const page = await open("dark", two([71, 96], [18, 93]));
    await sheet(page);
    await page.waitForSelector(".acc-banner");
    assertEqual(await text(page, ".acc-banner-head"), "Both accounts are close to their weekly limit");
    assert((await text(page, ".acc-banner-body")).startsWith("Home has used 96% and Work 93%. When both run out, the team pauses until Home is fresh again "), await text(page, ".acc-banner-body"));
    assert((await text(page, '.acc-card[data-id="1"] .acc-meter[data-k="week"] .acc-meter-when')).startsWith("Almost used up. Fresh again "), "the near meter says so");
    assert(await page.locator('.acc-card[data-id="1"] .acc-meter-when.is-near').isVisible(), "in gold");
    await page.click('.acc-banner [data-act="add-open"]');
    await page.waitForSelector("#acc-add");
    assertEqual(await text(page, ".acc-add-title"), "Add another Claude account");
    assertEqual(await texts(page, ".acc-field"), ["The new account"]);
    await page.close();
    return "banner and Add another account";
  });

  await run.check("the 5-hour choice is saved, and choosing turns the automatic pause or switch on", async () => {
    const page = await open("dark", two([58, 72], [0, 12]));
    await sheet(page);
    assertEqual(await page.locator('#accounts-sheet [role="radio"][aria-checked="true"]').count(), 0, "nothing is chosen while the automatic pause is off");
    await page.click("#acc-five-switch");
    await page.waitForSelector(".acc-saved");
    assertEqual(await page.locator("#acc-five-switch").getAttribute("aria-checked"), "true");
    const v = await page.evaluate(() => window.RichBridge.invoke("claude_quota", {}));
    assertEqual([v.policy.enabled, v.atThreshold], [true, "switch"]);
    assertEqual(await text(page, "#acc-five-switch .acc-opt-t"), "Switch to Work");
    await page.close();
    return "Switch to Work saved; the policy is on";
  });

  await run.check("Remove asks once, then says it is removed", async () => {
    const page = await open("dark", two([58, 72], [0, 12]));
    await sheet(page);
    await page.click('[aria-label="Remove Work"]');
    assert((await text(page, ".acc-confirm")).startsWith("Remove Work from RichOS? RichOS signs out of it here. Nothing changes on the account itself and you can add it back any time."));
    await page.click("#acc-remove-yes");
    await page.waitForSelector(".acc-feedback");
    assertEqual(await text(page, ".acc-feedback"), "Work is removed from RichOS. You can add it back any time.");
    assertEqual(await texts(page, ".acc-name"), ["Home"]);
    await page.close();
    return "removed";
  });

  // ---- every state, both themes: contrast, type size, fit, no dash ---------------------------
  const STATES = [
    ["menu-one", one(41), async p => { await menu(p); }],
    ["menu-near", one(86), async p => { await menu(p); await p.waitForFunction(() => document.querySelector("#set-accounts-state.is-attention")); }],
    ["one", one(86, 47), sheet],
    ["add", one(86, 47), async p => { await sheet(p); await p.click(".acc-slot"); await p.click("#acc-continue"); }],
    ["add-signin", one(86, 47), async p => { await sheet(p); await p.click(".acc-slot"); await p.fill("#acc-nu", "Work"); await p.click("#acc-continue"); }],
    ["add-wait", one(86, 47), async p => { await sheet(p); await p.click(".acc-slot"); await p.fill("#acc-nu", "Work"); await p.click("#acc-continue");
      await p.evaluate(() => { window.RichBridge.invoke = (orig => (cmd, a) => cmd === "claude_account_sign_in_poll" ? Promise.resolve(["2", { state: "connecting" }, null]) : orig(cmd, a))(window.RichBridge.invoke.bind(window.RichBridge)); });
      await p.click("#acc-open-signin"); await p.waitForSelector(".acc-waiting"); }],
    ["add-same", one(86, 47), async p => { await sheet(p); await p.click(".acc-slot"); await p.fill("#acc-nu", "Work"); await p.click("#acc-continue"); await p.click("#acc-open-signin"); await p.waitForSelector("#acc-try-again"); }, { signIn: ["same-account"] }],
    ["two", two([58, 72], [0, 12]), async p => { await sheet(p); await p.click('[aria-label="Use Work now"]'); await p.waitForSelector(".acc-feedback"); }],
    ["remove", two([58, 72], [0, 12]), async p => { await sheet(p); await p.click('[aria-label="Remove Work"]'); }],
    ["switched", two([64, 99], [6, 4], { inUse: "2", homeGone: now + 2 * DAY, lastSwitch: { from: "1", to: "2", at: now - 3 * 60000, why: "weekly", used: 99 } }), sheet],
    ["both-near", two([71, 96], [18, 93]), sheet],
    ["moving", two([64, 99], [6, 4], { inUse: "2", working: { "1": ["team"], "2": ["rich"] } }), sheet],
    ["menu-moving", two([64, 99], [6, 4], { inUse: "2", working: { "1": ["team"], "2": ["rich"] } }), async p => { await menu(p); await p.waitForFunction(() => document.querySelector("#set-accounts-state.is-attention")); }],
    ["choice-saved", two([58, 72], [0, 12]), async p => { await sheet(p); await p.click("#acc-five-pause"); await p.waitForSelector(".acc-saved"); }],
    ["menu-switched", two([64, 99], [6, 4], { inUse: "2", lastSwitch: { from: "1", to: "2", at: now - 3 * 60000, why: "weekly", used: 99 } }), async p => { await menu(p); await p.waitForFunction(() => document.querySelector("#set-accounts-state.is-attention")); }],
    ["chat-nudge", one(86, 47), async p => { await onThread(p); await say(p, "nudge", "Your Claude account has used **86%** of this week's limit.");
      await p.waitForSelector('[data-note-act="add"]'); }],
    ["chat-not-now", one(86, 47), async p => { await onThread(p); await say(p, "nudge", "Your Claude account has used **86%** of this week's limit.");
      await p.waitForSelector('[data-note-act="not-now"]'); await p.click('[data-note-act="not-now"]'); await p.waitForSelector(".acc-after"); }],
    ["chat-switched", two([20, 96], [6, 4]), async p => { await onThread(p); await say(p, "switched", "I switched the team to your **Work** account.");
      await p.waitForSelector('[data-note-act="see"]'); }],
    ["chat-both", two([71, 96], [18, 93]), async p => { await onThread(p); await say(p, "bothNear", "Heads up: both of your Claude accounts are close to their weekly limit.");
      await p.waitForSelector('[data-note-act="see"]'); }],
  ];
  // The roots measured: the sheet, the Settings row, and the slot under Rich's line.
  const ROOTS = "#accounts-sheet:not([hidden]) .acc-panel, #set-accounts-open, .acc-note-slot";
  // Declared skippable (style.css's own note): the 14px "Settings" breadcrumb over the 32px title.
  const SKIPPABLE = ".acc-eyebrow";
  for (const theme of ["dark", "light"]) {
    await run.check(theme + ": every round 18 state meets AA contrast and the type floor, fits, and carries no m-dash or n-dash", async () => {
      const measured = [];
      for (const [name, fixture, drive, opts] of STATES) {
        // Contrast in both themes at 1440 by 900; the fit at the app's minimum window, 1024 by 700, once
        // (the layout is the same in both themes).
        for (const viewport of theme === "dark" ? [{ width: 1440, height: 900 }, { width: 1024, height: 700 }] : [{ width: 1440, height: 900 }]) {
          const page = await open(theme, fixture, { ...(opts || {}), viewport });
          await drive(page);
          await page.evaluate(() => Promise.all(document.getAnimations().filter(a => a.effect && a.effect.getTiming().iterations !== Infinity).map(a => a.finished.catch(() => null))));
          await page.addScriptTag({ content: contrast.pageScript() });
          const result = await page.evaluate(({ ROOTS, SKIPPABLE }) => {
            const C = window.__contrastMath, failures = [], dashes = [];
            let worst = 99, nodes = 0;
            const ground = el => { let bg = { r: 0, g: 0, b: 0, a: 0 }; const chain = []; for (let p = el; p; p = p.parentElement) chain.unshift(p);
              for (const p of chain) bg = C.compositeOver(C.parseCssColor(getComputedStyle(p).backgroundColor), bg); return bg; };
            for (const root of document.querySelectorAll(ROOTS)) {
              if (!root.getClientRects().length) continue;
              if (/[–—]/.test(root.innerText)) dashes.push(root.innerText.match(/.{0,30}[–—].{0,30}/)[0]);
              for (const e of [root, ...root.querySelectorAll("*")]) {
                if (!e.getClientRects().length || e.closest("[hidden]")) continue;
                const own = [...e.childNodes].some(n => n.nodeType === Node.TEXT_NODE && n.textContent.trim());
                if (!own && e.tagName !== "INPUT") continue;
                const style = getComputedStyle(e), bg = ground(e), fg = C.compositeOver(C.parseCssColor(style.color), bg);
                const size = parseFloat(style.fontSize), large = size >= 24 || size >= 18.66 && parseInt(style.fontWeight) >= 700;
                const ratio = C.round2(C.contrastRatio(fg, bg)); nodes++;
                worst = Math.min(worst, ratio);
                if (ratio < (large ? 3 : 4.5)) failures.push({ text: (e.value || e.textContent).trim().slice(0, 50), ratio });
                if (size < 16 && !e.closest(SKIPPABLE)) failures.push({ text: e.textContent.trim().slice(0, 50), size });
              }
              // Non-text indicators, 3:1 against the ground they sit on: the in-use spine and border,
              // the meters' fill, the radio ring, the mini bar, a pill's ring.
              // A bar's fill is read against the card or row it sits on, not against its own track.
              for (const sel of [".acc-card.is-inuse .acc-spine", ".acc-meter-bar b", ".acc-radio", ".acc-mini i", ".acc-pill i", ".acc-btn:not(.acc-btn-quiet):not(.acc-btn-primary)"]) {
                for (const e of root.querySelectorAll(sel)) {
                  if (!e.getClientRects().length) continue;
                  const cs = getComputedStyle(e);
                  const paint = cs.backgroundColor !== "rgba(0, 0, 0, 0)" ? cs.backgroundColor : cs.borderTopColor;
                  const on = e.matches(".acc-meter-bar b, .acc-mini i") ? e.parentElement.parentElement : e.parentElement;
                  const bg = ground(on), ratio = C.round2(C.contrastRatio(C.compositeOver(C.parseCssColor(paint), bg), bg));
                  if (ratio < 3) failures.push({ indicator: sel, ratio });
                }
              }
            }
            const body = document.querySelector("#accounts-sheet:not([hidden]) .acc-body");
            const sideways = body ? body.scrollWidth > body.clientWidth + 1 : false;
            return { failures, dashes, worst, nodes, sideways };
          }, { ROOTS, SKIPPABLE });
          assertEqual(result.failures, [], `${theme} ${name} ${viewport.width}: contrast and type`);
          assertEqual(result.dashes, [], `${theme} ${name}: no m-dash or n-dash`);
          assert(!result.sideways, `${theme} ${name} ${viewport.width}: never scrolls sideways`);
          assert(result.nodes > 0, `${theme} ${name}: EMPTY INVENTORY`);
          if (viewport.width === 1440) measured.push(`${name} ${result.worst}:1`);
          // Every control is reachable, by scrolling where the window is small.
          const controls = page.locator("#accounts-sheet:not([hidden]) button, #accounts-sheet:not([hidden]) input");
          for (let i = 0, n = await controls.count(); i < n; i++) {
            const control = controls.nth(i);
            await control.scrollIntoViewIfNeeded(); assert(await control.isVisible(), `${theme} ${name} ${viewport.width}: control ${i} is out of reach`);
          }
          await page.close();
        }
      }
      return "worst text per state: " + measured.join(", ");
    });
  }

  await run.check("no renderer errors", async () => assertEqual(errors, []));
  await browser.close();
  process.exitCode = run.report() ? 1 : 0;
}
main().catch(e => { console.error(e); process.exitCode = 1; });

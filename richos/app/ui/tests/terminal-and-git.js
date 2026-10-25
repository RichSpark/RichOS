// TERMINAL SETUP and YOUR NAME IN GIT — richos-hq design round 41, both pieces, driven through
// the REAL SHELL: `index.html`, `main.js`, `settings-button.js`, `timeline.js`,
// `terminal-setup.js`, `git-name.js`, `terminal-and-git.css` and `style.css`, loaded from disk,
// with nothing stubbed but the Tauri bridge (which `mock.js` replaces with the shell's answers,
// computed the way `login_environment.rs` and `git_name.rs` compute them).
//
// The CEO, 2026-10-25: "Build #14 and #15". Built as drawn from round 41
// (`terminal-and-git-name.html` and its README):
//
//   1. Terminal setup is the LAST row under "Working with Rich", with the time it was last read.
//      Its sheet: Read it again ("Reading…", cannot be pressed twice), what it found (tool
//      folders in order, settings by name only, never a value), what changed (each change with a
//      mark and a word as well as a color), nothing changed, the read that did not finish (the
//      last good read is kept), and the start read that never finished.
//   2. Your name in Git: asked once, as a question card under Rich's reply, with the name filled
//      in and an email; Save or Use "RichOS" instead; the wrong email and the save that could not
//      be written; answered, it is never asked again; changed later under Your company and team,
//      below Connected folders; a user whose own Git setup names them is never asked.
//   3. Contrast, computed in BOTH themes off WebKit's resolved colors, and the 16px floor.
//   4. The committed pictures, `shots-41/*-{dark,light}.png`, published through
//      `publishShotFile`: written only under `RICHOS_SHOTS_REGENERATE`, otherwise compared.
//
// Run: node terminal-and-git.js

"use strict";

const path = require("path");
const {
  loadPlaywright,
  leaveHome,
  createRun,
  assert,
  assertEqual,
  shot,
  publishShotFile,
  openThread,
  FRAME_STILL,
  UI_DIR,
} = require("./lib/harness");
const contrast = require("./lib/contrast");

const APP = "file://" + path.join(UI_DIR, "index.html");
const SHOTS = path.join(__dirname, "shots-41");
const FIXED = new Date("2026-10-21T09:52:00-04:00");
const time = (ms) => new Date(ms).toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit", timeZone: "America/New_York" });

async function openApp(browser, opts) {
  opts = opts || {};
  const context = await browser.newContext({
    viewport: { width: 1440, height: 900 },
    timezoneId: "America/New_York",
    locale: "en-US",
  });
  const page = await context.newPage();
  await page.clock.setFixedTime(FIXED);
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push("console: " + m.text());
  });
  const theme = opts.theme || "dark";
  await page.addInitScript((t) => {
    try {
      window.localStorage.setItem("richos-theme", t);
      window.localStorage.setItem("richos-font-scale", "100");
      window.localStorage.setItem("richos-mock-config", JSON.stringify({ theme: t, font_scale: 100, user_name: null }));
    } catch (e) {
      /* storage unavailable: the shipped default */
    }
  }, theme);
  await page.addInitScript(FRAME_STILL);
  await page.addInitScript(() => {
    let state = 0x6a09e667;
    Math.random = () => {
      state ^= state << 13;
      state ^= state >>> 17;
      state ^= state << 5;
      return (state >>> 0) / 4294967296;
    };
  });
  await page.addInitScript((v) => {
    window.__RICHOS_MOCK_PRESET__ = v;
  }, opts.preset || {});
  await page.goto(APP);
  await leaveHome(page);
  await page.waitForSelector(".nav-thread", { state: "attached" });
  await openThread(page, "acme");
  page.__errors = errors;
  return page;
}

async function openSettings(page) {
  if (!(await page.isVisible("#set-menu"))) await page.click("#set-btn");
  await page.waitForSelector("#set-menu", { state: "visible" });
}

/// The rows of a Settings column, by id, in order, its label, and the row's line once it is read.
async function column(page, rowId) {
  await page.waitForFunction((id) => {
    const line = document.querySelector("#" + id + " .set-row-state");
    return !line || line.textContent.length > 0;
  }, rowId);
  return page.evaluate((id) => {
    const row = document.getElementById(id);
    const col = row.closest(".sm-col");
    return {
      group: col.querySelector(".sm-group").textContent,
      rows: [...col.querySelectorAll("button.bugbtn--disclosure")].map((b) => b.id),
      state: (row.querySelector(".set-row-state") || {}).textContent || "",
    };
  }, rowId);
}

async function openTerminal(page) {
  await openSettings(page);
  await page.click("#set-terminal-open");
  await page.waitForSelector("#terminal-setup", { state: "visible" });
  await page.waitForFunction(() => document.querySelector("#ts-read") && !document.querySelector("#ts-read").disabled);
}

/// The sheet as the user reads it.
async function terminalSheet(page) {
  return page.evaluate(() => {
    const q = (s) => document.querySelector("#terminal-setup " + s);
    const shown = (n) => !!n && !n.closest("[hidden]");
    return {
      tone: q("#ts-status").dataset.tone,
      main: q("#ts-main").textContent,
      sub: q("#ts-sub").textContent,
      button: q("#ts-read").textContent,
      disabled: q("#ts-read").disabled,
      changesShown: shown(q("#ts-changes")),
      changesTitle: q("#ts-changes-title").textContent,
      changesWhen: q("#ts-changes-when").textContent,
      changes: [...document.querySelectorAll("#ts-change-rows .tg-change")].map((r) =>
        [r.dataset.kind, r.querySelector(".tg-change-mark").textContent, r.querySelector(".tg-change-what").textContent, r.querySelector(".tg-change-name").textContent]),
      folders: [...document.querySelectorAll("#ts-folders li")].map((l) => l.textContent),
      settings: [...document.querySelectorAll("#ts-settings > *")].map((c) => c.textContent),
      lit: [...document.querySelectorAll("#terminal-setup .is-new")].map((n) => n.textContent),
      counts: [q("#ts-folders-count").textContent, q("#ts-settings-count").textContent],
      text: q(".tg-sheet").textContent,
    };
  });
}

const START = time(FIXED.getTime() - 38 * 60 * 1000); // the mock's start read, 38 minutes earlier

/// The card in the conversation, as the user reads it.
async function card(page) {
  return page.evaluate(() => {
    const c = document.getElementById("git-name-card");
    if (!c) return null;
    const v = (s) => (c.querySelector(s) ? c.querySelector(s).value : null);
    return {
      state: c.dataset.state,
      set: c.querySelector(".ask-set").textContent,
      question: c.querySelector(".ask-q").textContent,
      lede: (c.querySelector(".tg-ask-lede") || {}).textContent || null,
      folder: (c.querySelector(".tg-ask-lede b") || {}).textContent || null,
      name: v("#git-name-name"),
      email: v("#git-name-email"),
      placeholder: c.querySelector("#git-name-email") ? c.querySelector("#git-name-email").placeholder : null,
      focused: document.activeElement ? document.activeElement.id : null,
      invalid: c.querySelector("#git-name-email") ? c.querySelector("#git-name-email").getAttribute("aria-invalid") : null,
      nameInvalid: c.querySelector("#git-name-name") ? c.querySelector("#git-name-name").getAttribute("aria-invalid") : null,
      message: (c.querySelector(".tg-field-msg") || {}).textContent || null,
      problem: (c.querySelector(".tg-problem") || {}).textContent || null,
      wait: (c.querySelector(".tg-ask-wait") || {}).textContent || null,
      save: (c.querySelector("#git-name-save") || {}).textContent || null,
      skip: (c.querySelector("#git-name-skip") || {}).textContent || null,
      done: (c.querySelector(".tg-done-main") || {}).textContent || null,
      doneSub: (c.querySelector(".tg-done-sub") || {}).textContent || null,
      underReply: (() => {
        const prose = [...document.querySelectorAll(".tl-rich .tl-prose")].filter((p) => p.textContent.includes("pricing page in northwind-site"));
        return prose.length === 1 && (prose[0].compareDocumentPosition(c) & Node.DOCUMENT_POSITION_FOLLOWING) !== 0;
      })(),
    };
  });
}

/// Every text and indicator of the round's pieces on screen, against the paint behind it.
async function measure(page, scope) {
  await page.addScriptTag({ content: contrast.pageScript() });
  return page.evaluate((sel) => {
    const C = window.__contrastMath, out = [];
    const ground = (el) => {
      let bg = { r: 0, g: 0, b: 0, a: 0 };
      const chain = [];
      for (let p = el; p; p = p.parentElement) chain.unshift(p);
      for (const p of chain) bg = C.compositeOver(C.parseCssColor(getComputedStyle(p).backgroundColor), bg);
      return bg;
    };
    const ratio = (color, el) => {
      const bg = ground(el);
      return C.round2(C.contrastRatio(C.compositeOver(C.parseCssColor(color), bg), bg));
    };
    for (const root of document.querySelectorAll(sel)) {
      for (const e of [root, ...root.querySelectorAll("*")]) {
        if (!e.getClientRects().length || e.closest("[hidden]")) continue;
        const s = getComputedStyle(e);
        if (s.visibility === "hidden" || s.opacity === "0") continue;
        const own = [...e.childNodes].some((n) => n.nodeType === Node.TEXT_NODE && n.textContent.trim());
        if (own && !e.matches("input")) {
          out.push({ what: e.textContent.trim().slice(0, 40), ratio: ratio(s.color, e), size: parseFloat(s.fontSize), need: 4.5 });
        }
        if (e.matches("input")) {
          if (e.value) out.push({ what: "field: " + e.value, ratio: ratio(s.color, e), size: parseFloat(s.fontSize), need: 4.5 });
          else if (e.placeholder) out.push({ what: "example: " + e.placeholder, ratio: ratio(getComputedStyle(e, "::placeholder").color, e), size: parseFloat(s.fontSize), need: 4.5 });
          out.push({ what: "field edge " + e.id, ratio: ratio(s.borderTopColor, e.parentElement), size: 16, need: 3 });
        }
        if (e.dataset.contrastRole === "indicator") {
          const color = e.matches(".tg-dot, .tg-done-check") ? s.backgroundColor : s.color;
          out.push({ what: "indicator " + e.className, ratio: ratio(color, e.parentElement), size: 16, need: 3 });
        }
        if (e.matches(".tg-status")) {
          out.push({ what: "status bar " + e.dataset.tone, ratio: ratio(getComputedStyle(e, "::before").backgroundColor, e), size: 16, need: 3 });
          out.push({ what: "status edge", ratio: ratio(s.borderTopColor, e.parentElement), size: 16, need: 3 });
        }
        if (e.matches(".tg-changes")) out.push({ what: "changes edge", ratio: ratio(s.borderTopColor, e.parentElement), size: 16, need: 3 });
        if (e.matches(".tg-chip")) out.push({ what: "chip edge " + e.textContent, ratio: ratio(s.borderTopColor, e.parentElement), size: 16, need: 3 });
      }
    }
    return out;
  }, scope);
}

function assertAA(all, theme, label) {
  const bad = all.filter((m) => m.ratio < m.need || m.size < 16);
  assertEqual(bad, [], theme + " " + label + ": below AA or the 16px floor");
  const low = (need) => Math.min(...all.filter((m) => m.need === need).map((m) => m.ratio));
  return label + " " + all.length + " measured, lowest text " + low(4.5) + ":1, lowest indicator " + low(3) + ":1";
}

async function main() {
  const run = createRun("Terminal setup and your name in Git — round 41 (real shell, WebKit)");
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();

  // ---- 1. TERMINAL SETUP ------------------------------------------------------------------
  await run.check("Terminal setup is the last row under Working with Rich, with the time it was read", async () => {
    const page = await openApp(browser);
    await openSettings(page);
    const col = await column(page, "set-terminal-open");
    assertEqual(col.group, "Working with Rich");
    assertEqual(col.rows[col.rows.length - 1], "set-terminal-open", "last in its column: " + col.rows);
    assertEqual(col.rows[col.rows.length - 2], "set-phone-open", "under Use Rich from your phone");
    assertEqual(col.state, "Read at " + START + ", when RichOS started");
    assertEqual(page.__errors, [], "the page reported errors");
    await page.context().close();
    return col.state;
  });

  await run.check("the sheet says what was read, from which shell, the folders in order and the settings by name only", async () => {
    const page = await openApp(browser);
    await openTerminal(page);
    const s = await terminalSheet(page);
    assertEqual([s.tone, s.main, s.sub, s.button], ["ok", "Read at " + START + ", when RichOS started", "From zsh (/bin/zsh), the way a new Terminal window starts.", "Read it again"]);
    assertEqual(s.folders.slice(0, 3), ["1/opt/homebrew/bin", "2/opt/homebrew/sbin", "3~/.cargo/bin"], "numbered in the order the Terminal looks");
    assertEqual(s.counts, ["10 folders", "32 settings"]);
    assert(s.settings.includes("GITHUB_TOKEN") && s.settings.includes("PATH"), "names are listed");
    assert(!s.text.includes("/sdk/moved") && !/=\S/.test(s.settings.join(" ")), "a value reached the sheet");
    assert(s.text.includes("Names only. Their values go to your team as they are and are never shown or kept."));
    assert(s.text.includes("Your team uses this from its next job. A job already running keeps the setup it started with."));
    assertEqual(s.changesShown, false, "nothing changed yet, so no What changed");
    assertEqual(await page.evaluate(() => document.activeElement.id), "ts-read", "Read it again is the default button");
    await page.context().close();
    return s.counts.join(", ");
  });

  await run.check("Read it again: Reading…, cannot be pressed twice, then what changed, each with a mark and a word", async () => {
    const page = await openApp(browser);
    await openTerminal(page);
    await page.evaluate(() => window.__RICHOS_MOCK__.terminalHold(true));
    await page.click("#ts-read");
    await page.waitForFunction(() => document.querySelector("#ts-read").disabled);
    const busy = await terminalSheet(page);
    assertEqual([busy.tone, busy.main, busy.button, busy.disabled], ["busy", "Reading your Terminal setup…", "Reading…", true]);
    assertEqual(busy.sub, "Starting zsh the way a new Terminal window does. This takes a few seconds, never more than 10.");
    await page.click("#ts-read", { force: true });
    await page.keyboard.press("Enter");
    const calls = await page.evaluate(() => window.__RICHOS_MOCK__.terminalCalls().filter((c) => c === "terminal_setup_read_again").length);
    assertEqual(calls, 1, "pressed again while reading, it read once");
    await page.evaluate(() => window.__RICHOS_MOCK__.terminalRelease());
    await page.waitForFunction(() => !document.querySelector("#ts-changes").hidden);
    const s = await terminalSheet(page);
    assertEqual([s.main, s.sub], ["Read again at " + time(FIXED.getTime()), "5 changes since " + START + ", listed below."]);
    assertEqual([s.changesTitle, s.changesWhen], ["5 changes", "since " + START]);
    assertEqual(s.changes, [
      ["new", "+", "New tool folder", "/opt/homebrew/opt/openjdk/bin"],
      ["gone", "−", "Tool folder gone", "~/.nvm/versions/node/v20.11.0/bin"],
      ["new", "+", "New setting", "JAVA_HOME"],
      ["changed", "~", "Changed setting", "ANDROID_HOMEits value is different now"],
      ["gone", "−", "Setting gone", "NVM_DIR"],
    ]);
    assertEqual(s.lit, ["1/opt/homebrew/opt/openjdk/binNew", "ANDROID_HOME · changed", "JAVA_HOME · new"], "the new items are lit in the lists");
    const struck = await page.evaluate(() => [...document.querySelectorAll('.tg-change[data-kind="gone"] .tg-change-name')].map((n) => getComputedStyle(n).textDecorationLine));
    assertEqual(struck, ["line-through", "line-through"], "a gone name is struck through");
    await page.click("#ts-done");
    await openSettings(page);
    assertEqual((await column(page, "set-terminal-open")).state, "Read again at " + time(FIXED.getTime()) + " · 5 changes");
    assertEqual(page.__errors, [], "the page reported errors");
    await page.context().close();
    return s.changes.map((c) => c[1] + " " + c[2]).join(" | ");
  });

  await run.check("a second read that finds nothing new says so, in the sheet and on the row", async () => {
    const page = await openApp(browser);
    await openTerminal(page);
    await page.click("#ts-read");
    await page.waitForFunction(() => !document.querySelector("#ts-changes").hidden);
    await page.click("#ts-read");
    await page.waitForFunction(() => document.querySelector("#ts-main").textContent.endsWith("Nothing changed."));
    const s = await terminalSheet(page);
    assertEqual(s.changesShown, false);
    assert(s.sub.startsWith("It is the same as at " + time(FIXED.getTime()) + ". If you just changed something, open a new Terminal window"), s.sub);
    await page.keyboard.press("Escape");
    await page.waitForSelector("#terminal-setup", { state: "hidden" });
    assertEqual(await page.evaluate(() => document.activeElement.id), "set-btn", "Escape gives the focus back to Settings, whose row opened it");
    await openSettings(page);
    assertEqual((await column(page, "set-terminal-open")).state, "Read again at " + time(FIXED.getTime()) + " · nothing changed");
    await page.context().close();
    return s.main;
  });

  await run.check("the same tool folders in a new order are a change, with a mark and a word (review finding 2)", async () => {
    const page = await openApp(browser, { preset: { terminalReads: ["reorder"] } });
    await openTerminal(page);
    await page.click("#ts-read");
    await page.waitForFunction(() => !document.querySelector("#ts-changes").hidden);
    const s = await terminalSheet(page);
    assertEqual(s.changes, [["changed", "~", "Tool folders in a new order", "/opt/homebrew/sbinnow comes earlier, so a tool may come from it"]]);
    assertEqual(s.folders.slice(0, 2), ["1/opt/homebrew/sbin", "2/opt/homebrew/bin"], "listed in the new order");
    await page.click("#ts-done");
    await openSettings(page);
    assertEqual((await column(page, "set-terminal-open")).state, "Read again at " + time(FIXED.getTime()) + " · 1 change");
    await page.context().close();
    return s.changes[0].slice(1, 3).join(" ");
  });

  await run.check("a read that does not finish keeps the last good read and offers Try again", async () => {
    const page = await openApp(browser, { preset: { terminalReads: ["timeout"] } });
    await openTerminal(page);
    await page.click("#ts-read");
    await page.waitForFunction(() => document.querySelector("#ts-status").dataset.tone === "warn");
    const s = await terminalSheet(page);
    assertEqual([s.main, s.button], ["Your shell didn’t finish within 10 seconds", "Try again"]);
    assert(s.sub.startsWith("Nothing changed: your team keeps the setup read at " + START + "."), s.sub);
    assertEqual(s.counts, ["10 folders", "32 settings"], "the last good read is still listed");
    await page.click("#ts-done");
    await openSettings(page);
    assertEqual((await column(page, "set-terminal-open")).state, "Read at " + START + ", when RichOS started");
    await page.context().close();
    return s.main;
  });

  await run.check("a start read that never finished: the row warns, the lists are empty, Read it again fills them", async () => {
    const page = await openApp(browser, { preset: { terminal: "startfail", terminalReads: ["after"] } });
    await openSettings(page);
    const row = await column(page, "set-terminal-open");
    assertEqual(row.state, "Not read: your shell didn’t finish");
    assertEqual(await page.locator("#set-terminal-state svg").count(), 1, "with its warning triangle");
    await page.click("#set-terminal-open");
    await page.waitForSelector("#terminal-setup", { state: "visible" });
    await page.waitForFunction(() => document.querySelector("#ts-status").dataset.tone === "warn");
    const s = await terminalSheet(page);
    assertEqual(s.main, "Not read yet: your shell didn’t finish when RichOS started");
    assert(s.sub.startsWith("Until it is read, your team has only the Mac’s basic tools"), s.sub);
    assertEqual([s.folders, s.settings, s.counts], [["Nothing yet."], ["Nothing yet."], ["", ""]]);
    await page.click("#ts-read");
    await page.waitForFunction(() => document.querySelector("#ts-status").dataset.tone === "ok");
    const after = await terminalSheet(page);
    assertEqual([after.main, after.counts], ["Read at " + time(FIXED.getTime()), ["10 folders", "32 settings"]]);
    await page.context().close();
    return row.state + " → " + after.main;
  });

  // ---- 2. YOUR NAME IN GIT ------------------------------------------------------------------
  await run.check("the card is asked under Rich's reply, as the app's question card, with the name filled in", async () => {
    const page = await openApp(browser, { preset: { gitName: "ask" } });
    await page.waitForSelector("#git-name-card");
    const c = await card(page);
    assertEqual([c.state, c.set, c.question], ["open", "Asked once", "What name should go on your team’s saved work?"]);
    assertEqual(c.lede, "Your team is about to save its first change in northwind-site. Git, which keeps the history of that folder, puts a name and an email on every change. This Mac doesn’t have them set yet.");
    assertEqual([c.folder, c.name, c.email, c.placeholder], ["northwind-site", "Dana Whitfield", "", "you@yourcompany.com"]);
    assertEqual(c.message, "If you use GitHub, use your GitHub email so these changes show as yours.");
    assertEqual([c.wait, c.skip, c.save], ["Your team keeps working. Only this save waits for your answer.", "Use “RichOS” instead", "Save"]);
    assert(c.underReply, "the card is not under Rich's reply");
    const bar = await page.evaluate(() => document.getElementById("git-name-card").matches(".question-card.ask"));
    assert(bar, "drawn as the app's question card");
    // It counts as an open question like any other.
    await page.waitForFunction(() => /1 question open/.test(document.querySelector(".questions-open").textContent));
    assertEqual(page.__errors, [], "the page reported errors");
    await page.context().close();
    return c.question;
  });

  await run.check("an email that is not right is said in words beside a mark, typing clears it, an empty name refuses", async () => {
    const page = await openApp(browser, { preset: { gitName: "ask" } });
    await page.waitForSelector("#git-name-card");
    await page.fill("#git-name-email", "dana@");
    await page.click("#git-name-save");
    let c = await card(page);
    assertEqual([c.invalid, c.focused], ["true", "git-name-email"]);
    assertEqual(c.message, "That email isn’t complete. It needs an @ and a domain, like you@yourcompany.com.");
    assertEqual(await page.locator("#git-name-card .tg-field-msg svg").count(), 1, "beside a mark");
    await page.type("#git-name-email", "n");
    c = await card(page);
    assertEqual([c.invalid, c.message], [null, "If you use GitHub, use your GitHub email so these changes show as yours."], "typing clears it");
    await page.fill("#git-name-email", "");
    await page.focus("#git-name-email");
    await page.keyboard.press("Enter");
    c = await card(page);
    assertEqual(c.message, "Add an email. Git needs one with the name, like you@yourcompany.com.", "Return saves from the field");
    await page.fill("#git-name-name", "");
    await page.fill("#git-name-email", "dana@northwind.com");
    await page.click("#git-name-save");
    c = await card(page);
    assertEqual([c.nameInvalid, c.focused, c.state], ["true", "git-name-name", "open"]);
    assertEqual((await page.evaluate(() => window.__RICHOS_MOCK__.gitNameState())).choice, null, "nothing was saved");
    await page.context().close();
    return "three refusals, nothing saved";
  });

  await run.check("a save that could not be written says nothing changed, and Try again saves", async () => {
    const page = await openApp(browser, { preset: { gitName: "ask" } });
    await page.waitForSelector("#git-name-card");
    await page.evaluate(() => window.__RICHOS_MOCK__.gitNameFailNextSave());
    await page.fill("#git-name-email", "dana@northwind.com");
    await page.click("#git-name-save");
    await page.waitForSelector("#git-name-card .tg-problem");
    let c = await card(page);
    assertEqual([c.problem, c.save, c.state], ["RichOS couldn’t save this, so nothing changed and the save is still waiting. Try again.", "Try again", "open"]);
    assertEqual(c.email, "dana@northwind.com", "what was typed is kept");
    await page.click("#git-name-save");
    await page.waitForFunction(() => document.getElementById("git-name-card").dataset.state === "answered");
    c = await card(page);
    assertEqual(c.done, "Your team’s changes now carry Dana Whitfield, dana@northwind.com");
    await page.context().close();
    return c.done;
  });

  await run.check("Saved: the card settles, never asks again, and Change it in Settings opens the row", async () => {
    const page = await openApp(browser, { preset: { gitName: "ask" } });
    await page.waitForSelector("#git-name-card");
    await page.fill("#git-name-email", "dana@northwind.com");
    await page.click("#git-name-save");
    await page.waitForFunction(() => document.getElementById("git-name-card").dataset.state === "answered");
    let c = await card(page);
    assertEqual([c.done, c.doneSub, c.name], ["Your team’s changes now carry Dana Whitfield, dana@northwind.com", "RichOS won’t ask again. Change it in Settings", null]);
    assertEqual((await page.evaluate(() => window.__RICHOS_MOCK__.gitNameState())).choice, { choice: "own", name: "Dana Whitfield", email: "dana@northwind.com" });
    await page.waitForFunction(() => document.querySelector(".questions-open").hidden);
    // Never asked again: the conversation read again draws it answered, with no fields.
    await openThread(page, "hiring");
    await openThread(page, "acme");
    await page.waitForSelector("#git-name-card");
    c = await card(page);
    assertEqual([c.state, c.email], ["answered", null], "answered after the conversation is read again");
    await page.click("#git-name-change");
    await page.waitForSelector("#set-menu", { state: "visible" });
    assertEqual(await page.evaluate(() => document.activeElement.id), "set-git-open", "Settings opens on the row");
    const col = await column(page, "set-git-open");
    assertEqual(col.state, "Dana Whitfield · dana@northwind.com");
    await page.context().close();
    return col.state;
  });

  await run.check("Use RichOS: answered, never asked again, and Settings says until you set yours", async () => {
    const page = await openApp(browser, { preset: { gitName: "ask" } });
    await page.waitForSelector("#git-name-card");
    await page.click("#git-name-skip");
    await page.waitForFunction(() => document.getElementById("git-name-card").dataset.state === "answered");
    const c = await card(page);
    assertEqual([c.done, c.doneSub], ["Your team’s changes carry the name RichOS", "RichOS won’t ask again. Put your name on them in Settings"]);
    assertEqual((await page.evaluate(() => window.__RICHOS_MOCK__.gitNameState())).choice, { choice: "richos" });
    await openSettings(page);
    assertEqual((await column(page, "set-git-open")).state, "RichOS, until you set yours");
    await page.context().close();
    return c.done;
  });

  await run.check("the card the team's first save asks arrives at once, without a reload", async () => {
    const page = await openApp(browser);
    assertEqual(await page.locator("#git-name-card").count(), 0, "nothing asked before the first save");
    await page.evaluate(() => window.__RICHOS_MOCK__.gitNameAsk("acme", "northwind-site"));
    await page.waitForSelector("#git-name-card");
    const c = await card(page);
    assertEqual([c.state, c.folder], ["open", "northwind-site"]);
    await page.context().close();
    return "asked live";
  });

  await run.check("Your name in Git sits under Connected folders; changing it later saves from its sheet", async () => {
    const page = await openApp(browser);
    await openSettings(page);
    const col = await column(page, "set-git-open");
    assertEqual(col.group, "Your company and team");
    assertEqual(col.rows[col.rows.indexOf("set-repositories-open") + 1], "set-git-open", "directly under Connected folders: " + col.rows);
    assertEqual(col.state, "Not set yet");
    await page.click("#set-git-open");
    await page.waitForSelector("#git-name-sheet", { state: "visible" });
    await page.waitForFunction(() => document.activeElement && document.activeElement.id === "gn-email");
    const read = () => page.evaluate(() => ({
      lede: document.querySelector("#gn-edit .tg-lede").textContent,
      name: document.getElementById("gn-name").value,
      note: document.getElementById("gn-note").textContent,
      cancel: document.getElementById("gn-cancel").textContent,
    }));
    let s = await read();
    assertEqual(s.lede, "Git puts this name and email on every change your team saves in your connected folders. Anyone who can see those folders, on GitHub for example, sees them.");
    assertEqual([s.name, s.note, s.cancel], ["Dana Whitfield", "Kept in RichOS’s own settings. Your Terminal’s Git setup is not changed.", "Cancel"]);
    await page.fill("#gn-email", "dana@northwind.com");
    await page.click("#gn-save");
    await page.waitForFunction(() => document.getElementById("gn-note").textContent.startsWith("Saved."));
    s = await read();
    assertEqual([s.note, s.cancel], ["Saved. Your team’s next saved change carries it.", "Done"]);
    await page.click("#gn-cancel");
    await openSettings(page);
    assertEqual((await column(page, "set-git-open")).state, "Dana Whitfield · dana@northwind.com");
    await page.context().close();
    return "Not set yet → saved from Settings";
  });

  await run.check("when the user's own Git setup names them, nothing is asked and Settings shows it read only", async () => {
    const page = await openApp(browser, { preset: { gitName: "config" } });
    await page.evaluate(() => window.__RICHOS_MOCK__.gitNameAsk("acme", "northwind-site"));
    assertEqual(await page.locator("#git-name-card").count(), 0, "no card for a user whose Git names them");
    await openSettings(page);
    assertEqual((await column(page, "set-git-open")).state, "Dana Whitfield, from your Git setup");
    await page.click("#set-git-open");
    await page.waitForSelector("#gn-own", { state: "visible" });
    const s = await page.evaluate(() => ({
      edit: !document.getElementById("gn-edit").hidden,
      main: document.getElementById("gn-own-main").textContent,
      sub: document.getElementById("gn-own-sub").textContent,
      lede: document.querySelector(".tg-own-lede").textContent,
    }));
    assertEqual([s.edit, s.main, s.sub], [false, "Dana Whitfield · dana@northwind.com", "From your own Git setup, ~/.gitconfig"]);
    assert(s.lede.startsWith("This is the name your Terminal’s Git uses"), s.lede);
    await page.click("#gn-open-file");
    await page.click("#gn-finder");
    await page.waitForFunction(() => window.__RICHOS_MOCK__.gitNameState().actions.length === 2);
    assertEqual((await page.evaluate(() => window.__RICHOS_MOCK__.gitNameState())).actions, ["open", "reveal"]);
    await page.context().close();
    return s.main;
  });

  // ---- 3. CONTRAST, both themes, and 4. the committed pictures -------------------------------
  for (const theme of ["dark", "light"]) {
    await run.check(theme + " — every word and indicator in both pieces clears WCAG AA at 16px or more", async () => {
      const lines = [];
      let page = await openApp(browser, { theme });
      await openSettings(page);
      lines.push(assertAA(await measure(page, "#set-terminal-open, #set-git-open"), theme, "Settings rows"));
      await page.click("#set-terminal-open");
      await page.waitForFunction(() => !document.querySelector("#ts-read").disabled);
      await page.evaluate(() => window.__RICHOS_MOCK__.terminalHold(true));
      await page.click("#ts-read");
      await page.waitForFunction(() => document.querySelector("#ts-read").disabled);
      lines.push(assertAA(await measure(page, "#terminal-setup .tg-sheet"), theme, "Reading"));
      await page.evaluate(() => window.__RICHOS_MOCK__.terminalRelease());
      await page.waitForFunction(() => !document.querySelector("#ts-changes").hidden);
      await page.evaluate(() => Promise.all(document.getAnimations().map((a) => a.finished.catch(() => null))));
      lines.push(assertAA(await measure(page, "#terminal-setup .tg-sheet"), theme, "What changed"));
      await page.context().close();

      page = await openApp(browser, { theme, preset: { terminal: "startfail", terminalReads: ["timeout"] } });
      await openSettings(page);
      lines.push(assertAA(await measure(page, "#set-terminal-open"), theme, "Not read row"));
      await page.click("#set-terminal-open");
      await page.waitForFunction(() => document.querySelector("#ts-status").dataset.tone === "warn");
      lines.push(assertAA(await measure(page, "#terminal-setup .tg-sheet"), theme, "Not read at start"));
      await page.context().close();

      page = await openApp(browser, { theme, preset: { gitName: "ask" } });
      await page.waitForSelector("#git-name-card");
      await page.locator("#git-name-card").scrollIntoViewIfNeeded();
      lines.push(assertAA(await measure(page, "#git-name-card"), theme, "Card"));
      await page.fill("#git-name-email", "dana@");
      await page.click("#git-name-save");
      lines.push(assertAA(await measure(page, "#git-name-card"), theme, "Email not right"));
      await page.evaluate(() => window.__RICHOS_MOCK__.gitNameFailNextSave());
      await page.fill("#git-name-email", "dana@northwind.com");
      await page.click("#git-name-save");
      await page.waitForSelector("#git-name-card .tg-problem");
      lines.push(assertAA(await measure(page, "#git-name-card"), theme, "Couldn't save"));
      await page.click("#git-name-save");
      await page.waitForFunction(() => document.getElementById("git-name-card").dataset.state === "answered");
      lines.push(assertAA(await measure(page, "#git-name-card"), theme, "Saved"));
      await openSettings(page);
      await page.click("#set-git-open");
      await page.waitForSelector("#git-name-sheet", { state: "visible" });
      lines.push(assertAA(await measure(page, "#git-name-sheet .tg-sheet"), theme, "Change it later"));
      await page.context().close();

      page = await openApp(browser, { theme, preset: { gitName: "config" } });
      await openSettings(page);
      await page.click("#set-git-open");
      await page.waitForSelector("#gn-own", { state: "visible" });
      lines.push(assertAA(await measure(page, "#git-name-sheet .tg-sheet"), theme, "From your Git setup"));
      await page.context().close();
      return lines.join(" | ");
    });

    await run.check(theme + " — the committed pictures: what changed, the card, and the two rows", async () => {
      const done = [];
      let page = await openApp(browser, { theme });
      await openTerminal(page);
      await page.click("#ts-read");
      await page.waitForFunction(() => !document.querySelector("#ts-changes").hidden);
      await page.evaluate(() => Promise.all(document.getAnimations().map((a) => a.finished.catch(() => null))));
      const changes = await shot(page, "terminal-changes-" + theme, { fullPage: false, parkPointer: true });
      publishShotFile(changes.file, path.join(SHOTS, "terminal-changes-" + theme + ".png"));
      done.push("terminal-changes-" + theme);
      await page.context().close();

      page = await openApp(browser, { theme, preset: { gitName: "ask" } });
      await page.waitForSelector("#git-name-card");
      await page.locator("#git-name-card").scrollIntoViewIfNeeded();
      await page.evaluate(() => document.activeElement && document.activeElement.blur());
      const ask = await shot(page, "git-ask-" + theme, { fullPage: false, parkPointer: true });
      publishShotFile(ask.file, path.join(SHOTS, "git-ask-" + theme + ".png"));
      done.push("git-ask-" + theme);
      await openSettings(page);
      await page.evaluate(() => document.activeElement && document.activeElement.blur());
      const rows = await shot(page, "settings-rows-" + theme, { fullPage: false, parkPointer: true });
      publishShotFile(rows.file, path.join(SHOTS, "settings-rows-" + theme + ".png"));
      done.push("settings-rows-" + theme);
      assertEqual(page.__errors, [], "the page reported errors");
      await page.context().close();
      return done.map((n) => "shots-41/" + n + ".png").join(", ");
    });
  }

  await browser.close();
  process.exitCode = run.report() > 0 ? 1 : 0;
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

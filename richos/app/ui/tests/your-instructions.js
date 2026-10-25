// YOUR INSTRUCTIONS — round 36 screen 1 (§138, build plan item 1b), driven through the REAL SHELL:
// `index.html`, `main.js`, `settings-button.js`, `timeline.js`, `your-instructions.js`, `mock.js`
// and `style.css`, loaded from disk, with nothing stubbed but the Tauri bridge (which `mock.js`
// replaces).
//
// The CEO, 2026-10-21: "Implement this simplified version for the app." Settings → Working with
// Rich → "Your instructions", first in the column with its one plain line; the sheet that edits
// the user's own `instructions.md`, saved as they type; "Open the file" and "Show in Finder"; Done,
// Escape or a click outside closes it; a new user's empty editor with three example lines and
// "Have Rich ask me", which goes to the Onboarding conversation; and Rich's "Your instructions ·
// Saved" card, whose "See your instructions" opens the sheet with those lines lit in gold.
//
// One change the CEO approved on top of the round (2026-10-21 12:17:30Z): the saved line says Rich
// follows the change from the user's next NEW conversation (an open one keeps its instructions).
//
// THE FIXTURES are `mock.js`'s: by default a returning user's file with the round's six rules;
// `{ yourInstructions: "" }` a new user's empty file; `{ instructionsCard: "round-36" }` the
// round's Onboarding conversation with Rich's card for four rules.
//
// COMMITTED PICTURES, `shots-36/instructions-{settings,sheet,tip-every,tip-tech,empty,card,lit}-{dark,light}.png`,
// published through `publishShotFile`: written only under `RICHOS_SHOTS_REGENERATE`, otherwise
// compared.
//
// Run: node your-instructions.js

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
const { parseCssColor, compositeOver, contrastRatio, hex } = require("./lib/contrast");

const APP = "file://" + path.join(UI_DIR, "index.html");
const SHOTS = path.join(__dirname, "shots-36");
const FIXED = new Date("2026-10-21T09:05:00-04:00");
const SAVED = "Saved. Rich follows the change from your next new conversation.";
const ROW = "Your instructions";

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
  // Every command the page asks of the shell, in order, for a check to read.
  await page.addInitScript(() => {
    window.__asked = [];
    const wrap = () => {
      const bridge = window.RichBridge;
      if (!bridge || bridge.__wrapped) return;
      const invoke = bridge.invoke.bind(bridge);
      bridge.invoke = (cmd, args, o) => {
        window.__asked.push({ cmd, args: JSON.parse(JSON.stringify(args || {})) });
        return invoke(cmd, args, o);
      };
      bridge.__wrapped = true;
    };
    document.addEventListener("DOMContentLoaded", wrap);
  });
  await page.goto(APP);
  await leaveHome(page);
  await page.waitForSelector(".nav-thread", { state: "attached" });
  page.__errors = errors;
  return page;
}

async function openSettings(page) {
  await page.click("#set-btn");
  await page.waitForSelector("#set-instructions-open");
  await page.waitForFunction(() => document.getElementById("set-instructions-state").textContent !== "");
}

async function openSheet(page) {
  await openSettings(page);
  await page.click("#set-instructions-open");
  await page.waitForFunction(() => {
    const s = document.getElementById("your-instructions");
    return s && !s.hidden && !document.getElementById("yi-editor").readOnly;
  });
}

const sheetShown = (page) => page.evaluate(() => {
  const s = document.getElementById("your-instructions");
  return !!s && !s.hidden;
});

async function main() {
  const run = createRun("Your instructions — round 36 screen 1 (real shell, WebKit)");
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();

  await run.check("Settings: Your instructions is first in Working with Rich, with its one plain line", async () => {
    const page = await openApp(browser);
    await openSettings(page);
    const got = await page.evaluate(() => {
      const col = document.querySelector('section.sm-col[aria-labelledby="set-group-rich"]');
      const kids = [...col.children];
      const row = document.getElementById("set-instructions-open");
      return {
        firstAfterLabel: kids[1] === row,
        thenRule: kids[2] && kids[2].classList.contains("sm-divider"),
        name: row.querySelector(".set-name").textContent,
        line: row.querySelector(".set-row-state").textContent,
        lineShown: getComputedStyle(row.querySelector(".set-row-state")).display !== "none",
        lineSize: getComputedStyle(row.querySelector(".set-row-state")).fontSize,
        chevron: !!row.querySelector("svg"),
      };
    });
    assert(got.firstAfterLabel, "the row is not first in the column");
    assert(got.thenRule, "no rule under the row");
    assertEqual(got.name, ROW);
    assertEqual(got.line, "6 rules, followed in every company");
    assert(got.lineShown, "the line is not shown");
    assert(parseFloat(got.lineSize) >= 16, "the line is meant to be read: " + got.lineSize);
    assert(got.chevron, "the row has no chevron saying it opens a sheet");
    assertEqual(page.__errors, [], "the page reported errors");
    await page.context().close();
    return got.name + " · " + got.line;
  });

  await run.check("the sheet: what it is, the user's own file, saved as they type", async () => {
    const page = await openApp(browser);
    await openSheet(page);
    const seen = await page.evaluate(() => {
      const q = (s) => document.querySelector(s);
      const ask = q("#your-instructions .yi-ask");
      return {
        settingsClosed: q("#set-menu").hidden,
        title: q("#yi-title").textContent,
        chip: q(".yi-everywhere").textContent,
        lede: q("#yi-lede").textContent,
        text: q("#yi-editor").value,
        focused: document.activeElement === q("#yi-editor"),
        path: q("#yi-path").textContent,
        file: q(".yi-file").innerText.replace(/\s+/g, " ").trim(),
        note: q(".yi-note").innerText.trim(),
        save: q("#yi-save").textContent,
        askShown: getComputedStyle(ask).display !== "none",
        sheet: q("#your-instructions").innerText,
      };
    });
    assert(seen.settingsClosed, "Settings stayed open under the sheet");
    assertEqual(seen.title, ROW);
    assertEqual(seen.chip, "Every company");
    assertEqual(
      seen.lede,
      "Rich follows these in full, every time*, in every company: when he talks with you and when he directs your team. Write them in your own words. There is no length limit."
    );
    assert(!/\bStu\b/.test(seen.sheet), "the sheet names Stu outside the CEO's technical note");
    assertEqual(seen.note, "*", "the asterisk's line under the lede, with its ⓘ");
    assertEqual(seen.text, (await page.evaluate(() => window.__RICHOS_MOCK__.yourInstructions().text)), "the editor holds the file");
    assert(seen.focused, "the editor does not have focus");
    assertEqual(seen.path, "~/myrichos/instructions.md");
    assertEqual(seen.file, "Kept in your own file, ~/myrichos/instructions.md Open the file Show in Finder");
    assertEqual(seen.save, SAVED, "the saved line on open");
    assert(!seen.askShown, "a returning user is offered Have Rich ask me");

    await page.keyboard.type("- Call me before 9 am only if a customer is waiting.\n");
    await page.waitForFunction(() => document.getElementById("yi-save").textContent === "Saving…");
    await page.waitForFunction((s) => document.getElementById("yi-save").textContent === s, SAVED);
    const after = await page.evaluate(() => ({ editor: document.getElementById("yi-editor").value, mock: window.__RICHOS_MOCK__.yourInstructions() }));
    assertEqual(after.mock.text, after.editor, "the file holds what is in the editor");
    assert(after.mock.text.endsWith("- Call me before 9 am only if a customer is waiting.\n"), "the typed rule is not at the end of the file");
    assert(after.mock.saves.length >= 1 && after.mock.saves.length <= 3, "saves while typing one line: " + after.mock.saves.length);

    await page.click("#yi-open");
    await page.click("#yi-reveal");
    await page.waitForFunction(() => window.__RICHOS_MOCK__.yourInstructions().opened.length === 2);
    assertEqual(
      (await page.evaluate(() => window.__RICHOS_MOCK__.yourInstructions().opened)),
      [{ name: "instructions.md", reveal: false }, { name: "instructions.md", reveal: true }],
      "Open the file and Show in Finder ask the shell for the user's instructions file"
    );

    await page.click("#yi-done");
    assert(!(await sheetShown(page)), "Done did not close the sheet");
    await openSettings(page);
    await page.waitForFunction(() => document.getElementById("set-instructions-state").textContent === "7 rules, followed in every company");
    assertEqual(page.__errors, [], "the page reported errors");
    await page.context().close();
    return "typed one rule: saved, 7 rules";
  });

  await run.check("Done, Escape and a click outside each close the sheet, and closing saves what was typed", async () => {
    const page = await openApp(browser);
    await openSheet(page);
    await page.keyboard.press("Escape");
    await page.waitForFunction(() => document.getElementById("your-instructions").hidden);
    await openSheet(page);
    await page.mouse.click(30, 450);
    await page.waitForFunction(() => document.getElementById("your-instructions").hidden);
    await openSheet(page);
    // Typed and closed at once, before the save would have run: closing saves it.
    await page.keyboard.type("x");
    await page.click("#yi-done");
    await page.waitForFunction(() => window.__RICHOS_MOCK__.yourInstructions().text.endsWith("x"));
    // A click INSIDE the panel does not close it.
    await openSheet(page);
    await page.click("#yi-lede");
    assert(await sheetShown(page), "a click inside the panel closed it");
    assertEqual(page.__errors, [], "the page reported errors");
    await page.context().close();
  });

  await run.check("the file edited in another app shows in Settings and in the sheet the next time they open", async () => {
    const page = await openApp(browser);
    await page.evaluate(() => window.__RICHOS_MOCK__.yourInstructionsEditedElsewhere("- One rule only.\n"));
    await openSettings(page);
    await page.waitForFunction(() => document.getElementById("set-instructions-state").textContent === "1 rule, followed in every company");
    await page.click("#set-instructions-open");
    await page.waitForFunction(() => document.getElementById("yi-editor").value === "- One rule only.\n");
    await page.context().close();
  });

  await run.check("a refused save says so, in place of Saved", async () => {
    const page = await openApp(browser);
    await openSheet(page);
    await page.evaluate(() => window.__RICHOS_MOCK__.yourInstructionsFailSave("Your instructions could not be saved: the disk is full"));
    await page.keyboard.type("y");
    await page.waitForFunction(() => document.getElementById("yi-save").textContent.startsWith("That could not be saved"));
    assertEqual(
      await page.textContent("#yi-save"),
      "That could not be saved: Your instructions could not be saved: the disk is full"
    );
    await page.context().close();
  });

  await run.check("the two ⓘ: the CEO's words, word for word, shown on a click, and Esc puts away only the tooltip", async () => {
    const page = await openApp(browser);
    await openSheet(page);
    const read = (id) => page.evaluate((id) => {
      const tip = document.getElementById(id + "-tip");
      const sheet = document.querySelector(".yi-sheet").getBoundingClientRect();
      const box = tip.getBoundingClientRect();
      const n = (s) => s.replace(/\s+/g, " ").trim();
      return {
        shown: !tip.hidden,
        text: [...tip.children].flatMap((c) => c.tagName === "UL" ? [...c.children].map((li) => "- " + n(li.textContent)) : [n(c.textContent)]),
        bold: [...tip.querySelectorAll("strong")].map((b) => b.textContent),
        code: [...tip.querySelectorAll("code")].map((c) => c.textContent),
        inside: box.left >= sheet.left && box.right <= sheet.right && box.top >= sheet.top && box.bottom <= sheet.bottom,
        label: document.getElementById(id).getAttribute("aria-label"),
        sameIcon: document.getElementById(id).className === "cx-info" && document.getElementById(id).innerHTML === (document.querySelector("#cx-info") || document.getElementById(id)).innerHTML,
        sizes: [...tip.querySelectorAll("p, li, code, strong")].map((e) => parseFloat(getComputedStyle(e).fontSize)),
      };
    }, id);
    await page.click("#yi-every");
    await page.waitForFunction(() => !document.getElementById("yi-every-tip").hidden);
    const every = await read("yi-every");
    assertEqual(every.text, [
      "“Every time” is an ambition because it cannot be guaranteed. That has to do with fundamental limitations of modern-day AI.",
      "So, in practice, you have to expect that sometimes one of those rules will be ignored.",
      "RichOS is constantly developing new ways and updating the app to prevent that from happening as much as technically possible.",
    ]);
    assertEqual(every.bold, ["you have to expect that sometimes one of those rules will be ignored"], "the bold part");
    assert(every.inside, "the tooltip runs out of the sheet");
    assert(every.sizes.every((px) => px >= 16), "tooltip text under 16px: " + every.sizes);
    assertEqual(every.label, "About “every time”");
    assert(every.sameIcon, "the ⓘ is not Settings' own");
    // Esc puts away the tooltip and leaves the sheet open.
    await page.keyboard.press("Escape");
    await page.waitForFunction(() => document.getElementById("yi-every-tip").hidden);
    assert(await sheetShown(page), "Esc on the tooltip closed the sheet");
    await page.click("#yi-tech");
    await page.waitForFunction(() => !document.getElementById("yi-tech-tip").hidden);
    const tech = await read("yi-tech");
    assertEqual(tech.text, [
      "Info for technical people",
      "- At the start of each Rich or Stu session, the RichOS app makes a fresh copy of your rich.md and stu.md for that session. (Stu is Rich’s twin brother who works behind the scenes as the orchestrator of your AI team. Rich talks to you and makes sure the work gets done to spec.)",
      "- rich.md and stu.md each work like a CLAUDE.md or AGENTS.md for one agent. Each file is that agent’s own standing instructions.",
      "- instructions.md contains your shared extra rules for both. You write a rule once and Rich and Stu both follow it from their next session. So, rich.md and stu.md contain rules that are different for each at the top. And the contents of instructions.md are added to each of those files at the bottom under the heading “The user’s own instructions”.",
    ]);
    assertEqual(tech.code, ["rich.md", "stu.md", "rich.md", "stu.md", "CLAUDE.md", "AGENTS.md", "instructions.md", "rich.md", "stu.md", "instructions.md"], "file names set as code");
    assert(tech.inside, "the tooltip runs out of the sheet");
    assert(tech.sizes.every((px) => px >= 16), "tooltip text under 16px: " + tech.sizes);
    assertEqual(tech.label, "Info for technical people");
    // A press elsewhere (the sheet's "Every company", clear of the tooltip) puts it away; opening
    // an ⓘ never changes the file.
    await page.click(".yi-everywhere");
    await page.waitForFunction(() => document.getElementById("yi-tech-tip").hidden);
    assertEqual((await page.evaluate(() => window.__RICHOS_MOCK__.yourInstructions().saves)), [], "an ⓘ saved the file");
    assertEqual(page.__errors, [], "the page reported errors");
    await page.context().close();
  });

  await run.check("a new user: Not written yet, an empty editor with three examples, and Have Rich ask me goes to Onboarding", async () => {
    const page = await openApp(browser, { preset: { yourInstructions: "", onboardingConversation: true } });
    await openSettings(page);
    assertEqual(await page.textContent("#set-instructions-state"), "Not written yet");
    await page.click("#set-instructions-open");
    await page.waitForFunction(() => !document.getElementById("yi-editor").readOnly);
    const seen = await page.evaluate(() => {
      const q = (s) => document.querySelector(s);
      return {
        value: q("#yi-editor").value,
        hint: q("#yi-editor").placeholder,
        ask: getComputedStyle(q(".yi-ask")).display !== "none" ? q(".yi-ask p").textContent : null,
        button: q("#yi-ask-rich").textContent,
        save: q("#yi-save").textContent,
      };
    });
    assertEqual(seen.value, "");
    assertEqual(
      seen.hint,
      "For example:\n- Ask me before spending more than $500.\n- Show me any email to a customer before it goes out.\n- Start every report with the answer."
    );
    assertEqual(seen.ask, "Not sure what to write? Rich can ask you a few questions and write them down for you.");
    assertEqual(seen.button, "Have Rich ask me");
    assertEqual(seen.save, "Nothing written yet.");
    // Typing hides the offer; clearing it again brings it back.
    await page.keyboard.type("a");
    assert(await page.evaluate(() => getComputedStyle(document.querySelector(".yi-ask")).display === "none"), "the offer stayed while writing");
    await page.keyboard.press("Backspace");
    assert(await page.evaluate(() => getComputedStyle(document.querySelector(".yi-ask")).display !== "none"), "the offer did not come back");

    await page.click("#yi-ask-rich");
    await page.waitForFunction(() => document.getElementById("scope-thread").textContent === "Onboarding");
    assert(!(await sheetShown(page)), "the sheet stayed open");
    await page.waitForFunction(() => window.__asked.some((a) => a.cmd === "send_message"));
    const sent = await page.evaluate(() => window.__asked.filter((a) => a.cmd === "send_message").map((a) => a.args));
    assertEqual(sent.length, 1, "one message to Rich");
    assertEqual(sent[0].threadId ?? sent[0].thread_id, "onboarding", "it went to the Onboarding conversation");
    assertEqual(sent[0].pressed, true, "sent as the pressed button, never in the user's name");
    assertEqual(
      await page.evaluate(() => [...document.querySelectorAll(".tl-user-text")].map((n) => n.textContent)),
      [],
      "the words show as the user's own message"
    );
    assertEqual(page.__errors, [], "the page reported errors");
    await page.context().close();
    return "sent: " + sent[0].text;
  });

  // Review rv-20261025T015241Z-b4dae6d4-fbf3#1: a company's heading (or New chat in it) shows that
  // company while the conversation he came from keeps its scope underneath, so the button reads
  // the company on screen, never the conversation behind it.
  await run.check("Have Rich ask me goes to the Onboarding of the company on screen, not the conversation he came from", async () => {
    const page = await openApp(browser, { preset: { yourInstructions: "", onboardingConversation: true, secondOnboarding: "lumen" } });
    const pressFrom = async (where) => {
      await openThread(page, "partner");
      if (where) {
        await page.click(where);
        await page.waitForFunction(() => document.getElementById("entity-view-name").textContent === "Northwind Traders");
      }
      await page.evaluate(() => { window.__asked.length = 0; });
      await openSheet(page);
      await page.click("#yi-ask-rich");
      await page.waitForFunction(() => window.__asked.some((a) => a.cmd === "send_message"));
      const sent = await page.evaluate(() => window.__asked.filter((a) => a.cmd === "send_message").map((a) => a.args));
      assertEqual(sent.length, 1, "one message to Rich");
      return sent[0].threadId ?? sent[0].thread_id;
    };
    assertEqual(await pressFrom("#nav-label-northwind"), "onboarding", "from Northwind's heading, after a Lumen conversation");
    assertEqual(await pressFrom(null), "onboarding-lumen", "from the Lumen conversation itself");
    assertEqual(page.__errors, [], "the page reported errors");
    await page.context().close();
  });

  await run.check("Rich's card: Your instructions · Saved, his rules, and See your instructions lights them in the sheet", async () => {
    const page = await openApp(browser, { preset: { instructionsCard: "round-36" } });
    await openThread(page, "onboarding");
    await page.waitForSelector(".yi-card");
    const card = await page.evaluate(() => {
      const c = document.querySelector(".yi-card");
      const lane = c.closest(".tl-lane");
      const prose = [...lane.querySelectorAll(".tl-rich")];
      return {
        title: c.querySelector(".yi-card-title").textContent,
        saved: c.querySelector(".yi-card-saved").textContent,
        rules: [...c.querySelectorAll(".yi-card-rules li")].map((li) => li.textContent),
        foot: c.querySelector(".yi-card-foot p").textContent,
        button: c.querySelector(".yi-card-open").textContent,
        inReply: prose.length > 0 && prose[prose.length - 1].contains(c),
        reply: prose.length ? prose[prose.length - 1].textContent : "",
      };
    });
    assertEqual(card.title, ROW);
    assertEqual(card.saved, "Saved");
    assertEqual(card.rules, [
      "Ask me before spending more than $500 on anything.",
      "Never email a customer without showing me the draft first.",
      "Write in plain English. No jargon in anything a customer reads.",
      "Keep anything non-urgent until after 1 pm. Mornings are for deep work.",
    ]);
    assertEqual(card.foot, "Followed in every company.");
    assertEqual(card.button, "See your instructions");
    assert(card.inReply, "the card is not in Rich's reply");
    assert(card.reply.includes("I wrote those down as your instructions"), "the card is not under the reply that saved them");

    await page.click(".yi-card-open");
    await page.waitForFunction(() => !document.getElementById("your-instructions").hidden && !document.getElementById("yi-editor").readOnly);
    const lit = await page.evaluate(() => ({
      marks: [...document.querySelectorAll(".yi-glow mark")].map((m) => m.textContent),
      animation: document.querySelector(".yi-glow mark") ? getComputedStyle(document.querySelector(".yi-glow mark")).animationName : null,
    }));
    assertEqual(lit.marks, card.rules.map((r) => "- " + r), "the card's lines are lit in the sheet");
    assertEqual(lit.animation, "yi-mark");
    assertEqual(page.__errors, [], "the page reported errors");
    await page.context().close();
    return card.rules.length + " rules lit";
  });

  await run.check("a save that arrives live shows at once, its rules arriving one by one", async () => {
    const page = await openApp(browser, { preset: { onboardingConversation: true } });
    await openThread(page, "acme");
    const turn = await page.evaluate(() => [...document.querySelectorAll(".tl-turn")].at(-1).dataset.turnId);
    await page.evaluate((t) => window.__RICHOS_MOCK__.instructionsCardArrive("acme", t, ["From now on, copy me on every invoice."]), turn);
    await page.waitForSelector(".yi-card");
    const motion = await page.evaluate(() => {
      const li = document.querySelector(".yi-card-rules li");
      return { arriving: li.classList.contains("is-arriving"), name: getComputedStyle(li).animationName };
    });
    assert(motion.arriving, "the rule did not arrive");
    assertEqual(motion.name, "yi-rule-in");
    await page.emulateMedia({ reducedMotion: "reduce" });
    assertEqual(await page.evaluate(() => getComputedStyle(document.querySelector(".yi-card-rules li")).animationName), "none");
    assertEqual(page.__errors, [], "the page reported errors");
    await page.context().close();
  });

  for (const theme of ["dark", "light"]) {
    await run.check(theme + " — the sheet's and the card's text and edges clear WCAG AA", async () => {
      const lines = [];
      const solid = (c, under) => {
        const v = parseCssColor(c);
        return v.a < 1 ? compositeOver(v, under) : v;
      };
      const measure = (what, color, ground, floor) => {
        const fg = solid(color, ground);
        const ratio = Math.round(contrastRatio(fg, ground) * 100) / 100;
        lines.push(what + " " + hex(fg) + " on " + hex(ground) + " " + ratio + ":1");
        assert(ratio >= floor, theme + ": " + what + " is " + ratio + ":1 against " + floor + ":1");
      };
      const GROUND = parseCssColor(theme === "dark" ? "rgb(12, 19, 34)" : "rgb(234, 230, 221)");
      // The sheet, a returning user's, then a new user's.
      for (const preset of [{}, { yourInstructions: "" }]) {
        const page = await openApp(browser, { theme, preset });
        await openSheet(page);
        // The editor's edge at rest: focus paints it in the accent instead.
        await page.evaluate(() => document.activeElement && document.activeElement.blur());
        const r = await page.evaluate(() => {
          const cs = (s, p) => getComputedStyle(document.querySelector(s), p || null);
          return {
            panel: cs(".yi-sheet").backgroundColor, scrim: cs("#your-instructions").backgroundColor,
            title: cs(".yi-title").color, lede: cs(".yi-lede").color, ledeB: cs(".yi-lede b").color,
            chip: cs(".yi-everywhere").color, chipIcon: cs(".yi-everywhere svg").color,
            sunk: cs(".yi-editor-wrap").backgroundColor, editor: cs("#yi-editor").color,
            hint: cs("#yi-editor", "::placeholder").color, edge: cs("#yi-editor").borderTopColor,
            file: cs(".yi-file").color, code: cs(".yi-file code").color, link: cs("#yi-open").color,
            save: cs("#yi-save").color, note: cs(".yi-note").color, infoRest: cs("#yi-every").color, check: document.querySelector("#yi-save svg") ? cs("#yi-save svg").color : null,
            ask: cs(".yi-ask").backgroundColor, askText: cs(".yi-ask p").color,
            empty: document.getElementById("yi-editor").value === "",
          };
        });
        const panel = solid(r.panel, solid(r.scrim, GROUND));
        const sunk = solid(r.sunk, panel);
        measure("title", r.title, panel, 4.5);
        measure("lede", r.lede, panel, 4.5);
        measure("lede, bold", r.ledeB, panel, 4.5);
        measure("Every company", r.chip, panel, 4.5);
        measure("Every company icon (indicator)", r.chipIcon, panel, 3);
        measure("editor edge against the editor (indicator)", r.edge, sunk, 3);
        measure("editor edge against the panel (indicator)", r.edge, panel, 3);
        measure("file line", r.file, panel, 4.5);
        measure("file path", r.code, panel, 4.5);
        measure("Open the file / Show in Finder", r.link, panel, 4.5);
        measure("saved line", r.save, panel, 4.5);
        // The two ⓘ and their tooltips, each opened as a click opens it.
        for (const id of ["yi-every", "yi-tech"]) {
          await page.click("#" + id);
          await page.waitForFunction((id) => !document.getElementById(id + "-tip").hidden, id);
          const t = await page.evaluate((id) => {
            const tip = document.getElementById(id + "-tip");
            const one = (sel) => { const n = tip.querySelector(sel); return n ? getComputedStyle(n).color : null; };
            return { bg: getComputedStyle(tip).backgroundColor, p: one(".cx-p"), li: one("li"), strong: one("strong"), code: one("code"),
              icon: getComputedStyle(document.getElementById(id)).color };
          }, id);
          const tipGround = solid(t.bg, panel);
          measure(id + " ⓘ at rest (indicator)", r.infoRest, panel, 3);
          measure(id + " ⓘ open (indicator)", t.icon, panel, 3);
          if (t.p) measure(id + " tooltip text", t.p, tipGround, 4.5);
          if (t.li) measure(id + " tooltip list", t.li, tipGround, 4.5);
          if (t.strong) measure(id + " tooltip bold", t.strong, tipGround, 4.5);
          if (t.code) measure(id + " tooltip file name", t.code, tipGround, 4.5);
          await page.keyboard.press("Escape");
          await page.waitForFunction((id) => document.getElementById(id + "-tip").hidden, id);
        }
        measure("asterisk line", r.note, panel, 4.5);
        if (r.empty) {
          measure("example hint", r.hint, sunk, 4.5);
          const ask = solid(r.ask, panel);
          measure("Have Rich ask me offer", r.askText, ask, 4.5);
        } else {
          measure("editor text", r.editor, sunk, 4.5);
          if (r.check) measure("saved check (indicator)", r.check, panel, 3);
        }
        assertEqual(page.__errors, [], "the page reported errors");
        await page.context().close();
      }
      // The card.
      const page = await openApp(browser, { theme, preset: { instructionsCard: "round-36" } });
      await openThread(page, "onboarding");
      await page.waitForSelector(".yi-card");
      const c = await page.evaluate(() => {
        const cs = (s, p) => getComputedStyle(document.querySelector(s), p || null);
        let chat = null;
        for (let e = document.querySelector(".yi-card").parentElement; e; e = e.parentElement) {
          const bg = getComputedStyle(e).backgroundColor;
          if (bg && bg !== "rgba(0, 0, 0, 0)" && bg !== "transparent") { chat = bg; break; }
        }
        return {
          chat, card: cs(".yi-card").backgroundColor, edge: cs(".yi-card").borderTopColor,
          title: cs(".yi-card-title").color, saved: cs(".yi-card-saved").color, savedIcon: cs(".yi-card-saved svg").color,
          rule: cs(".yi-card-rules li").color, dot: cs(".yi-card-rules li", "::before").backgroundColor,
          footBg: cs(".yi-card-foot").backgroundColor, foot: cs(".yi-card-foot p").color,
        };
      });
      const chat = solid(c.chat || "rgba(0,0,0,0)", GROUND);
      const card = solid(c.card, chat);
      measure("card edge against the chat (indicator)", c.edge, chat, 3);
      measure("card title", c.title, card, 4.5);
      measure("card Saved", c.saved, card, 4.5);
      measure("card Saved check (indicator)", c.savedIcon, card, 3);
      measure("card rule", c.rule, card, 4.5);
      measure("card rule dot (indicator)", c.dot, card, 3);
      measure("card foot line", c.foot, solid(c.footBg, card), 4.5);
      await page.context().close();
      return lines.join(" | ");
    });

    await run.check(theme + " — the committed pictures: Settings, the sheet, a new user's, the card and its lit lines", async () => {
      const files = [];
      const take = async (page, name, sel) => {
        const s = await shot(page, name + "-" + theme, { fullPage: false, parkPointer: true, ...(sel ? {} : {}) });
        const dest = path.join(SHOTS, name + "-" + theme + ".png");
        publishShotFile(s.file, dest);
        files.push("shots-36/" + name + "-" + theme + ".png");
      };
      let page = await openApp(browser, { theme });
      await openThread(page, "acme");
      await openSettings(page);
      await take(page, "instructions-settings");
      await page.click("#set-instructions-open");
      await page.waitForFunction(() => !document.getElementById("yi-editor").readOnly);
      await page.evaluate(() => document.activeElement && document.activeElement.blur());
      await take(page, "instructions-sheet");
      // The CEO's two ⓘ, each open (pinned by a click, the pointer parked away from it).
      for (const [id, name] of [["yi-every", "instructions-tip-every"], ["yi-tech", "instructions-tip-tech"]]) {
        await page.click("#" + id);
        await page.waitForFunction((id) => document.getElementById(id + "-tip").classList.contains("is-shown"), id);
        await page.waitForTimeout(250);
        await take(page, name);
        await page.keyboard.press("Escape");
      }
      await page.context().close();
      page = await openApp(browser, { theme, preset: { yourInstructions: "" } });
      await openThread(page, "acme");
      await openSheet(page);
      await page.evaluate(() => document.activeElement && document.activeElement.blur());
      await take(page, "instructions-empty");
      await page.context().close();
      page = await openApp(browser, { theme, preset: { instructionsCard: "round-36" } });
      await openThread(page, "onboarding");
      await page.waitForSelector(".yi-card");
      await page.locator(".yi-card").scrollIntoViewIfNeeded();
      await take(page, "instructions-card");
      await page.click(".yi-card-open");
      await page.waitForFunction(() => !document.getElementById("yi-editor").readOnly);
      // The lines at their brightest (12% into the 3.2 s glow), held still for the picture.
      await page.evaluate(() => {
        for (const a of document.getAnimations()) {
          if (a.animationName === "yi-mark") { a.pause(); a.currentTime = 384; }
        }
      });
      await take(page, "instructions-lit");
      assertEqual(page.__errors, [], "the page reported errors");
      await page.context().close();
      return files.join(", ");
    });
  }

  await browser.close();
  process.exitCode = run.report() > 0 ? 1 : 0;
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

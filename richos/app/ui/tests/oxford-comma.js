// THE OXFORD COMMA IN WHAT CLAUDE WRITES, AND THE SWITCH FOR IT (CEO, 2026-10-19).
//
// His words, in order: "make the Oxford comma linter switchable in settings. Default setting:
// Oxford comma off. Explainer line underneath: `Oxford comma AKA serial comma is what some style
// guides place before "and" and "or". RichOS recommends to keep that nuisance off.`"; "for the
// output the user gets from Claude, the user should be able to choose whether or not they want
// that nuisance"; and for the row's heading: "In the off setting change this heading: "Oxford
// comma" to "Oxford comma: OFF" and in the on setting "Oxford comma: ON"".
//
// Driven through the real shell (`index.html` with the browser mock standing in for the Mac), the
// real live events `main.js` listens to, the real Settings panel and the mock's durable config,
// which is this harness's stand-in for config.rs:
//
//   1  OFF BY DEFAULT: a fresh install draws Rich's reply and the question Claude wrote on a card
//      with the comma before and/or/nor deleted. A code span keeps its comma. The stored message
//      is untouched (the filter is applied where the text is drawn, never to what is kept).
//   2  THE ROW, as round 33 draws it: directly under Text size after a rule, the label "Oxford
//      comma: OFF" on an unchecked switch, his explainer word for word under it.
//   3  SWITCHING ON TAKES EFFECT AT ONCE: the label reads "Oxford comma: ON", the reply and the
//      card on screen show the comma again with no reload, and the choice is saved.
//   4  IT IS KEPT ACROSS A RELAUNCH, and switching back off deletes the comma at once again.
//   5  THE APP'S OWN TEXT stays without it whatever the switch says: the switch filters only
//      what Claude wrote, so a line the app wrote with "and" in it is drawn as written.
//
// Run: node oxford-comma.js

"use strict";

const path = require("path");
const { loadPlaywright, createRun, assert, assertEqual, UI_DIR, leaveHome, bootSettled } = require("./lib/harness");

const APP = "file://" + path.join(UI_DIR, "index.html");
const REPLY = "Red, green, and blue. Tea, or coffee? Run `a, and b` first.";
const REPLY_OFF = "Red, green and blue. Tea or coffee? Run a, and b first.";
const REPLY_ON = "Red, green, and blue. Tea, or coffee? Run a, and b first.";
const HINT = 'Oxford comma AKA serial comma is what some style guides place before "and" and "or". RichOS recommends to keep that nuisance off.';

/// The shell, with the live-event handlers `main.js` registers captured so a reply and a question
/// card can be put on screen exactly the way the Mac sends them. `context` is a relaunch: a page in
/// the same context keeps the mock config the last one saved.
async function openApp(context) {
  const page = await context.newPage();
  page.__errors = [];
  page.on("pageerror", (e) => page.__errors.push(String(e)));
  await page.addInitScript(() => {
    Object.defineProperty(window, "RichBridge", {
      configurable: true,
      set(bridge) {
        window.fixtureHandlers = {};
        const listen = bridge.listen.bind(bridge);
        bridge.listen = (name, fn) => {
          window.fixtureHandlers[name] = fn;
          return listen(name, fn);
        };
        Object.defineProperty(window, "RichBridge", { value: bridge, writable: true, configurable: true });
      },
    });
  });
  await page.goto(APP);
  await leaveHome(page);
  await bootSettled(page);
  await page.evaluate(() => openThread("general"));
  await page.evaluate(
    ({ reply }) => {
      const at = Date.now();
      const fence = {
        threadId: activeThreadId,
        entityId: timelineModel.entityId,
        bindingRevision: timelineModel.bindingRevision,
        turnId: "oxford-turn",
        visibility: "ceo",
      };
      fixtureHandlers["rich://turn-status"]({ payload: { ...fence, status: "working", at, startedAt: at } });
      fixtureHandlers["rich://message-started"]({ payload: { ...fence, messageId: "oxford-reply", seq: 0, slot: "stream", createdAt: at, at } });
      fixtureHandlers["rich://message-delta"]({ payload: { ...fence, messageId: "oxford-reply", seq: 0, textDelta: reply, at } });
      fixtureHandlers["rich://message-completed"]({ payload: { ...fence, messageId: "oxford-reply", seq: 0, text: reply, at } });
      const question = {
        id: "oxford-question",
        text: "Ship today, or tomorrow?",
        options: [
          { id: "today", label: "Today, or never", description: "Fixes, tests, and notes" },
          { id: "tomorrow", label: "Tomorrow", description: "More testing" },
        ],
        state: "open",
        revision: 0,
      };
      fixtureHandlers["rich://question-upserted"]({
        payload: { ...fence, turnId: "oxford-ask", id: "oxford-question", kind: "question", question, sequence: 1, slot: "stream", createdAt: at + 1, at: at + 1 },
      });
    },
    { reply: REPLY }
  );
  await page.locator('[data-question-id="oxford-question"] .question-option').first().waitFor();
  await frames(page);
  return page;
}

const frames = (page) => page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));

async function onScreen(page) {
  return page.evaluate(() => {
    const card = document.querySelector('[data-question-id="oxford-question"]');
    return {
      reply: (document.getElementById("prose:oxford-reply") || {}).textContent || "",
      code: Array.from(document.querySelectorAll('[id="prose:oxford-reply"] code')).map((n) => n.textContent),
      stored: timelineModel.items.get("oxford-reply").text,
      question: card ? card.querySelector(".question-title").textContent : "",
      label: card ? card.querySelector(".question-label").textContent : "",
      description: card ? card.querySelector(".question-description").textContent : "",
    };
  });
}

async function openMenu(page) {
  await page.click("#set-btn");
  await page.waitForSelector("#set-menu", { state: "visible" });
}

async function row(page) {
  return page.evaluate(() => {
    const r = document.getElementById("set-oxford-row");
    if (!r) return null;
    const prev = r.previousElementSibling;
    const before = prev ? prev.previousElementSibling : null;
    const next = r.nextElementSibling;
    return {
      label: document.getElementById("set-oxford-label").textContent,
      checked: document.getElementById("set-oxford").checked,
      describedBy: document.getElementById("set-oxford").getAttribute("aria-describedby"),
      hint: next && next.id === "oxford-hint" ? next.textContent : null,
      hintClass: next ? next.className : null,
      ruleAbove: !!(prev && prev.classList.contains("sm-divider")),
      aboveRule: before ? (before.querySelector(".set-name") || {}).textContent || "" : "",
      stored: JSON.parse(localStorage.getItem("richos-mock-config") || "{}").oxford_comma,
    };
  });
}

async function main() {
  const run = createRun("The Oxford comma in what Claude writes, and its switch (CEO 2026-10-19)");
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();
  const context = await browser.newContext({ viewport: { width: 1440, height: 900 }, colorScheme: "dark" });
  let page = await openApp(context);

  await run.check("1  off by default: Rich's reply and the question card are drawn without it; code and the stored text keep it", async () => {
    const s = await onScreen(page);
    assertEqual(s.reply, REPLY_OFF, "the reply as drawn");
    assertEqual(s.code, ["a, and b"], "the code span keeps its comma");
    assertEqual(s.stored, REPLY, "the stored message is what Claude wrote, never rewritten");
    assertEqual([s.question, s.label, s.description], ["Ship today or tomorrow?", "Today or never", "Fixes, tests and notes"], "the question card");
    return JSON.stringify(s.reply);
  });

  await run.check("2  the row, as round 33 draws it: under Text size after a rule, \"Oxford comma: OFF\", his explainer under it", async () => {
    await openMenu(page);
    const r = await row(page);
    assert(r, "there is no Oxford comma row in Settings");
    assertEqual(r.label, "Oxford comma: OFF", "the heading while off");
    assertEqual(r.checked, false, "the switch is off by default");
    assertEqual(r.hint, HINT, "his explainer, word for word, directly under the row");
    assertEqual(r.hintClass, "popover-hint", "in the panel's hint style");
    assertEqual(r.describedBy, "oxford-hint", "the switch is described by the explainer");
    assert(r.ruleAbove && r.aboveRule === "Text size", "directly under Text size, after a rule: " + JSON.stringify(r));
    return r.label;
  });

  await run.check("3  switching it on takes effect at once, says \"Oxford comma: ON\", and is saved", async () => {
    await page.click("#set-oxford");
    await frames(page);
    const r = await row(page);
    assertEqual([r.label, r.checked, r.stored], ["Oxford comma: ON", true, true], "the row and the saved choice");
    const s = await onScreen(page);
    assertEqual(s.reply, REPLY_ON, "the reply on screen shows the comma again with no reload");
    assertEqual([s.question, s.label, s.description], ["Ship today, or tomorrow?", "Today, or never", "Fixes, tests, and notes"], "the question card");
    return JSON.stringify(s.reply);
  });

  await run.check("4  his choice survives a relaunch, and switching it off deletes the comma at once again", async () => {
    await page.close();
    page = await openApp(context);
    let s = await onScreen(page);
    assertEqual(s.reply, REPLY_ON, "after the relaunch, still on");
    await openMenu(page);
    let r = await row(page);
    assertEqual([r.label, r.checked], ["Oxford comma: ON", true], "the row after the relaunch");
    await page.click("#set-oxford");
    await frames(page);
    r = await row(page);
    assertEqual([r.label, r.checked, r.stored], ["Oxford comma: OFF", false, false], "switched back off");
    s = await onScreen(page);
    assertEqual(s.reply, REPLY_OFF, "the comma is gone again at once");
    return "on across the relaunch, then off";
  });

  await run.check("5  the app's own text is not Claude's: the switch never touches a line the app wrote", async () => {
    // A line the app itself shows in the conversation (an app status notice) is drawn exactly as
    // written in both states; the commit-time fixer and dialect.js keep the app's own text clean.
    const shown = await page.evaluate(() => {
      const before = window.RichOxford.keeping();
      const host = document.createElement("div");
      document.body.appendChild(host);
      const own = "Saved here, and kept.";
      window.RichOxford.setKeeping(false);
      window.RichTimeline.renderMarkdownInto(host, own);
      const off = host.textContent;
      window.RichOxford.setKeeping(true);
      window.RichTimeline.renderMarkdownInto(host, own);
      const on = host.textContent;
      window.RichOxford.setKeeping(before);
      host.remove();
      return [off, on];
    });
    assertEqual(shown, ["Saved here, and kept.", "Saved here, and kept."], "the renderer itself filters nothing");
    assertEqual(page.__errors, [], "no page error");
    return "the filter is applied only where Claude's words are drawn";
  });

  await context.close();
  await browser.close();
  return run.report();
}

main().then((f) => process.exit(f ? 1 : 0));

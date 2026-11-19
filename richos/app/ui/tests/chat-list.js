"use strict";

const path = require("node:path");
const { loadPlaywright, createRun, assert, assertEqual, UI_DIR, leaveHome, shellSettled } = require("./lib/harness");
const contrast = require("./lib/contrast");

const app = "file://" + path.join(process.env.RICHOS_CHAT_LIST_BASE_UI || UI_DIR, "index.html");

async function pageFor(browser, theme) {
  const page = await browser.newPage({ colorScheme: theme, reducedMotion: "reduce", viewport: { width: 1440, height: 900 } });
  page.setDefaultTimeout(5000);
  await page.addInitScript((theme) => {
    window.localStorage.setItem("richos-theme", theme);
    let bridge;
    Object.defineProperty(window, "RichBridge", {
      get: () => bridge,
      set(value) {
        bridge = value;
        const invoke = value.invoke.bind(value);
        const now = Date.now();
        const base = {
          general: { working_since: now - 120_000, team_working: true, detail: "Team · running · Opus" },
          acme: { unread: true, seen_at: now - 86_400_000, result_at: now - 1000 },
          hiring: { question_count: 2, waiting_since: now - 180_000, stopped_at: now - 2, working_since: now - 2000 },
          ecs: { paused: true, working_since: now - 100_000, resume_at: now + 3_600_000 },
          partner: { stopped_at: now - 60_000, failure_at: now - 60_000 },
          memory: {},
        };
        const read = (name) => JSON.parse(localStorage.getItem("chat-list-proof-" + name) || "{}");
        const write = (name, payload) => localStorage.setItem("chat-list-proof-" + name, JSON.stringify(payload));
        const facts = (id) => ({ seen_at: now - 200_000, ...base[id], ...read("states")[id] });
        window.chatFacts = async (id, bits) => {
          write("states", { ...read("states"), [id]: { ...facts(id), ...bits } });
          await refreshNavigation();
        };
        value.invoke = async (command, args) => {
          if (command === "navigation_tree") {
            const tree = await invoke(command, args);
            for (const group of tree.groups) for (const row of group.threads) row.chat = facts(row.id);
            return tree;
          }
          if (command === "nav_state") {
            const nav = await invoke(command, args);
            nav.collapsed_entities = ["northwind"];
            Object.assign(nav, read("notifications"));
            return nav;
          }
          if (command === "set_chat_notifications") {
            const patch = args.mac === undefined ? { chat_notification_sounds: args.sounds } : { mac_chat_notifications: args.mac };
            write("notifications", { ...read("notifications"), ...patch });
            return null;
          }
          if (command === "set_chat_list_state") {
            const chat = facts(args.threadId);
            if (args.action === "later") chat.later_until = args.until;
            if (args.action === "now") chat.later_until = null;
            if (["seen", "read"].includes(args.action)) {
              chat.seen_at = args.seenAt || Date.now();
              chat.unread = false;
              if (args.action === "seen") chat.back_at = null;
            }
            if (args.action === "unread") chat.unread = true;
            write("states", { ...read("states"), [args.threadId]: chat });
            if (args.action === "seen") void refreshNavigation();
            return null;
          }
          return invoke(command, args);
        };
      },
    });
  }, theme);
  await page.goto(app);
  await leaveHome(page);
  await shellSettled(page);
  return page;
}

const railRow = (page, id) => page.locator("#rail-nav [data-thread-id=" + id + "]").first();
const markOf = async (page, id) => {
  const mark = railRow(page, id).locator(".chat-word");
  return (await mark.count()) ? (await mark.textContent()) : null;
};
const more = (page, id) => railRow(page, id).locator("..").locator(".nav-thread-more");
const set = (page, id, state) => page.evaluate(([thread, bits]) => chatFacts(thread, bits), [id, state]);
async function menu(page, id, label) {
  await more(page, id).focus();
  await more(page, id).click();
  await page.getByRole("menuitem", { name: label, exact: true }).click();
}

async function run() {
  const run = createRun("Chat list states, reminders and notifications");
  const browser = await loadPlaywright().webkit.launch();
  for (const theme of ["light", "dark"]) {
    const page = await pageFor(browser, theme);
    await run.check(theme + " · priority, cross-company order and durable sources", async () => {
      const group = page.locator(".nav-group--needs [data-thread-id]");
      assertEqual(await group.evaluateAll((items) => items.map((el) => el.dataset.threadId)), ["hiring", "partner"], "oldest wait first, including the collapsed company");
      assertEqual(await markOf(page, "hiring"), "questions", "needs you outranks stopped and working");
      assertEqual(await markOf(page, "partner"), "Stopped", "failure is named");
      assertEqual(await markOf(page, "general"), "Working", "a team job keeps the mark after Rich's reply");
      assert((await markOf(page, "ecs")).startsWith("Paused · "), "next time is shown in plain view");
      assertEqual(await railRow(page, "ecs").locator(".chat-list-sub").count(), 0, "ordinary rows stay one line");
      assertEqual(await railRow(page, "general").locator(".chat-shape svg, svg.chat-shape").count(), 1, "the word has its glyph");
      await set(page, "hiring", { question_count: 0, waiting_since: null });
      assertEqual(await markOf(page, "hiring"), "Stopped", "answering reveals a lower state");
      await set(page, "hiring", { stopped_at: null });
      assertEqual(await page.locator(".nav-group--needs [data-thread-id=hiring]").count(), 0, "resumed work leaves the attention group");
      await railRow(page, "ecs").click();
      await page.locator(".chat-band").waitFor({ state: "visible" });
      assert((await page.locator(".chat-band").textContent()).includes("Continues by itself at about"), "resume time reaches the composer");
      await set(page, "ecs", { paused: false, resume_at: null });
      assertEqual(await markOf(page, "ecs"), "Working", "quota reset returns the job to Working");
      await set(page, "ecs", { recovering: true });
      assert((await page.locator(".chat-band").textContent()).includes("picking this back up"));
      await set(page, "ecs", { recovering: false, working_since: null, unread: true });
      assertEqual(await markOf(page, "ecs"), "Working", "the underlying pending-turn fact remains until terminal");
    });

    await run.check(theme + " · archive, Undo and reminder actions", async () => {
      await menu(page, "general", "Archive");
      await page.locator(".chat-undo").waitFor();
      await page.locator("#rail-nav .nav-group--entity [data-thread-id=general]").waitFor({ state: "detached" });
      await page.locator(".chat-undo button").click();
      await railRow(page, "general").waitFor();
      await menu(page, "general", "Bring back later…");
      await page.getByRole("button", { name: "Pick a time…" }).click();
      assertEqual(await page.locator(".chat-pick-day").count(), 7, "the picker offers the next seven days");
      await page.getByRole("button", { name: "Bring it back then" }).click();
      await page.locator(".nav-group--later [data-thread-id=general]").waitFor();
      await menu(page, "general", "Bring back now");
      await page.locator(".nav-group--later [data-thread-id=general]").waitFor({ state: "detached" });
      await menu(page, "general", "Bring back later…");
      await page.getByRole("button", { name: /^Tomorrow morning/ }).click();
      await page.locator(".nav-group--later [data-thread-id=general]").waitFor();
      await page.keyboard.press("Control+z");
      await page.locator(".nav-group--later [data-thread-id=general]").waitFor({ state: "detached" });
      await set(page, "general", { working_since: null, back_at: Date.now() });
      assertEqual(await markOf(page, "general"), "Back", "a returned chat reaches Needs you");
    });

    await run.check(theme + " · unread persists and opening adds the unread boundary", async () => {
      await page.evaluate(() => {
        const id = "northwind";
        navPrefs.collapsed_entities = navPrefs.collapsed_entities.filter((x) => x !== id);
        renderRail();
      });
      assertEqual(await markOf(page, "acme"), "New", "a saved result is unread");
      await menu(page, "acme", "Mark as read");
      await page.waitForFunction(() => !document.querySelector("#rail-nav [data-thread-id=acme] .chat-mark"));
      await menu(page, "acme", "Mark as unread");
      await page.waitForFunction(() => document.querySelector("#rail-nav [data-thread-id=acme] .chat-mark--new"));
      await page.reload();
      await leaveHome(page);
      await shellSettled(page);
      await page.evaluate(() => { navPrefs.collapsed_entities = []; renderRail(); });
      assertEqual(await markOf(page, "acme"), "New", "manual unread survives a new page");
      await railRow(page, "acme").click();
      await page.locator(".chat-since").waitFor();
      await page.waitForFunction(() => !document.querySelector("#rail-nav [data-thread-id=acme] .chat-mark--new"));
    });

    await run.check(theme + " · settings and painted contrast", async () => {
      await page.click("#set-btn");
      const mac = page.getByRole("switch", { name: "Mac notifications" });
      const sound = page.getByRole("switch", { name: "Notification sounds" });
      assert(await mac.isChecked() && await sound.isChecked(), "both switches start enabled");
      await sound.uncheck();
      await page.waitForFunction(() => JSON.parse(localStorage.getItem("chat-list-proof-notifications")).chat_notification_sounds === false);
      assert(await mac.isChecked(), "sound is independent");
      await mac.uncheck();
      await page.waitForFunction(() => JSON.parse(localStorage.getItem("chat-list-proof-notifications")).mac_chat_notifications === false);
      await page.keyboard.press("Escape");
      await page.addScriptTag({ content: contrast.pageScript() });
      const ratios = await page.evaluate(() => {
        const { parseCssColor, compositeOver, contrastRatio } = window.__contrastMath;
        const elements = [...document.querySelectorAll(".chat-word, .chat-count, svg.chat-shape, .chat-list-sub, .chat-since")];
        return elements.filter((el) => el.getClientRects().length).map((el) => {
          const colors = [];
          for (let parent = el; parent; parent = parent.parentElement) {
            colors.unshift(parseCssColor(getComputedStyle(parent).backgroundColor));
          }
          const background = colors.reduce((beneath, top) => compositeOver(top, beneath), { r: 255, g: 255, b: 255, a: 1 });
          return { name: el.textContent || el.parentElement.className, ratio: contrastRatio(parseCssColor(getComputedStyle(el).color), background),
            floor: el.matches("svg") ? 3 : 4.5, size: parseFloat(getComputedStyle(el).fontSize) };
        });
      });
      assert(ratios.length >= 10, "contrast sample includes real state marks");
      for (const sample of ratios) {
        assert(sample.ratio >= sample.floor, JSON.stringify(sample));
        if (sample.floor === 4.5) assert(sample.size >= 16, JSON.stringify(sample));
      }
      return ratios.length + " visible samples";
    });
    await page.close();
  }
  await browser.close();
  if (run.report()) process.exitCode = 1;
}

run().catch((error) => { console.error(error); process.exitCode = 1; });

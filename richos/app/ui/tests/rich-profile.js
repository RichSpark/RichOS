"use strict";

const path = require("path");
const { pathToFileURL } = require("url");
const { launchGuardedWebKit } = require("./lib/browser-watchdog");
const { assert, assertEqual, createRun, leaveHome, loadPlaywright, UI_DIR } = require("./lib/harness");

async function openApp(browser, theme, width, preset = null) {
  const context = await browser.newContext({
    viewport: { width, height: 900 }, locale: "en-US", timezoneId: "Etc/UTC",
    reducedMotion: "reduce",
  });
  const page = await context.newPage();
  await page.addInitScript((theme) => {
    localStorage.setItem("richos-theme", theme);
    localStorage.setItem("richos-mock-config", JSON.stringify({ theme, rich_model: "opus" }));
  }, theme);
  if (preset) await page.addInitScript((value) => { window.__RICHOS_MOCK_PRESET__ = value; }, preset);
  await page.goto(pathToFileURL(path.join(UI_DIR, "index.html")).href);
  await leaveHome(page);
  if (!preset) await page.locator(".tl-rich-meta .tl-avatar").first().waitFor();
  return page;
}

async function openFromGreeting(page) {
  const avatar = page.locator(".tl-rich-meta .tl-avatar").first();
  await avatar.click();
  const profile = page.locator("#rich-profile");
  assert(await profile.isVisible(), "clicking Rich's greeting portrait opens the profile");
  return profile;
}

async function main() {
  const run = createRun("Rich's profile");
  const browser = await launchGuardedWebKit(loadPlaywright().webkit, run);
  try {
    await run.check("the docked greeting profile in light and dark has Alex's copy and one continuous divider", async () => {
      for (const theme of ["light", "dark"]) {
        const page = await openApp(browser, theme, 1440);
        const profile = await openFromGreeting(page);
        assertEqual(await profile.locator("h2").innerText(), "Rich Hand", theme);
        assertEqual(await profile.locator(".rich-profile-title").innerText(), "Your AI Chief of Staff & Business Operations Lead", theme);
        assertEqual(await profile.locator(".rich-profile-about").innerText(), "I help you realize your strategic vision. Together with my twin brother Stu I manage a team of specialist AI teammates to get things done. So, I come back to you only when something needs you.", theme);
        assertEqual(await profile.locator("dt").filter({ hasText: "Works with" }).locator("+ dd").innerText(), "Stu (who takes the longer jobs) and the team", theme);
        assertEqual(await profile.locator("dt").filter({ hasText: "Reach him" }).locator("+ dd").innerText(), "Here by text, by voice or on your phone", theme);
        assertEqual(await profile.locator("dt").filter({ hasText: "Thinks with" }).locator("+ dd").innerText(), "Opus", theme);
        await profile.getByText("August 31, 2026", { exact: true }).waitFor();
        assertEqual(await page.evaluate(() => window.RichProfile.reportDetails()),
          "docked, first-launch date shown, current chats shown", theme + " bounded support details");
        const lines = await page.evaluate(() => {
          const stage = document.querySelector("#stage-header");
          const head = document.querySelector("#rich-profile header");
          const panel = document.querySelector("#rich-profile");
          const divider = (el) => {
            const css = getComputedStyle(el);
            const box = el.getBoundingClientRect();
            return [box.bottom, css.borderBottomWidth, css.borderBottomColor];
          };
          return {
            stage: divider(stage), profile: divider(head),
            gap: panel.getBoundingClientRect().left - stage.getBoundingClientRect().right,
            width: panel.getBoundingClientRect().width,
          };
        });
        assertEqual(lines.profile, lines.stage, theme + " divider coordinate, thickness and colour");
        assertEqual(lines.gap, 0, theme + " divider junction");
        assertEqual(lines.width, 400, theme + " panel width");
        const current = profile.locator(".rich-profile-now");
        await current.getByRole("button", { name: /ECS architecture.*Stu is on it/ }).waitFor();
        await current.getByRole("button", { name: /Partner book review.*Stopped/ }).click();
        await page.locator(".nav-thread[data-thread-id='partner'][aria-current='true']").first().waitFor();
        assert(await profile.isHidden(), "opening a chat closes the profile");
        await page.context().close();
      }
    });

    await run.check("reply names work by keyboard and the profile shares Output's dock and overlay", async () => {
      const page = await openApp(browser, "dark", 1440);
      await page.locator('.nav-thread[data-thread-id="acme"]').first().click();
      const reply = page.locator(".tl-rich-meta").last();
      await reply.getByText("Rich", { exact: true }).waitFor();
      const name = reply.getByRole("button", { name: "Rich Hand: open his profile" });
      assertEqual(await name.count(), 1, "a reply identity is a keyboard stop");
      await name.focus();
      await page.keyboard.press("Enter");
      const profile = page.locator("#rich-profile");
      await profile.waitFor();
      await page.keyboard.press("Escape");
      assert(await profile.isHidden(), "Escape closes the profile");
      assert(await name.evaluate((button) => button === document.activeElement), "Escape returns focus to Rich");
      await name.click();
      await page.locator("#out-top").click();
      assert(await profile.isHidden(), "Output takes the dock");
      await page.locator("#outpanel").waitFor();
      await name.click();
      assert(await page.locator("#outpanel").isHidden(), "the profile takes the dock back");
      await page.setViewportSize({ width: 1100, height: 900 });
      await page.locator("#rich-profile-scrim").waitFor();
      assertEqual(await profile.evaluate((panel) => getComputedStyle(panel).position), "fixed", "narrow window overlays");
      await page.locator("#rich-profile-scrim").click({ position: { x: 600, y: 200 } });
      assert(await profile.isHidden(), "the overlay scrim closes the profile");

      await page.evaluate(() => {
        const messages = document.querySelector("#messages");
        messages.append(window.RichSuiteTools.note(false, true));
        messages.append(window.RichQuestions.render({
          threadId: "acme",
          question: { id: "profile-test", state: "withdrawn", text: "Continue?", options: [], asker: "Stu" },
        }, { showIdentity: () => true, showAvatar: () => true }));
      });
      await page.locator(".suite-note .tl-avatar").click();
      await profile.waitFor();
      await page.keyboard.press("Escape");
      const question = page.locator(".question-row");
      assertEqual(await question.locator(".tl-who").first().textContent(), "Stu · through Rich", "the asker remains named");
      await question.getByRole("button", { name: "Rich Hand: open his profile" }).last().click();
      await profile.waitFor();
      await page.context().close();
    });

    await run.check("the setup portrait opens the profile above the setup sheet and Escape returns to Rich", async () => {
      const page = await openApp(browser, "light", 1440, { setup: "missing-both", dictation: true, videoTools: "downloading", memory: "ready" });
      await page.locator("#setup-go").click();
      await page.locator("#setup-steps").waitFor();
      await page.evaluate(() => window.__RICHOS_MOCK__.setupEmit({
        state: "started", component: "media-tools", what: "Getting my voice and video tools", index: 1,
        total: 1, received: 300000000, of: 1061655396, counter: "300 MB of 1.06 GB", both_models: true,
      }));
      await page.locator("#setup-rich-avatar").click();
      const profile = page.locator("#setup-sheet #rich-profile");
      assert(await profile.isVisible(), "clicking the portrait opens the setup profile");
      await page.keyboard.press("Escape");
      assert(await profile.isHidden(), "Escape dismisses the profile");
      assert(await page.locator("#setup-sheet").isVisible(), "Escape leaves setup open");
      assertEqual(await page.evaluate(() => document.activeElement.id), "setup-rich-name", "focus returns to Rich's name");
      await page.context().close();
    });
  } finally {
    await browser.close();
    if (run.report()) process.exitCode = 1;
  }
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});

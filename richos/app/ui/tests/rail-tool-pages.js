"use strict";

const path = require("path");
const { loadPlaywright, createRun, assert, assertEqual, UI_DIR,
  leaveHome, shellSettled, flushFrames, captureSettled, publishShot, SHOT_DIR } = require("./lib/harness");
const contrast = require("./lib/contrast");

async function open(browser, theme, available = true, delayTools = false) {
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, reducedMotion: "reduce" });
  await page.addInitScript(({ theme, available, delayTools }) => {
    localStorage.setItem("richos-theme", theme);
    localStorage.setItem("richos-font-scale", "100");
    localStorage.setItem("richos-mock-config", JSON.stringify({ theme, font_scale: 100, user_name: "Alex Booster" }));
    window.__RICHOS_MOCK_PRESET__ = { richFiles: true, meetingNotes: { available: !delayTools } };
    const state = {
      shortcut: { available, enabled: false, registered: false,
        shortcut: { code: "KeyC", control: true, shift: true, option: false, command: false } },
      workflow: { opensEditor: false, keepsHistory: false },
      viewer: { pictures: false },
    };
    const calls = [];
    window.glanceCalls = calls;
    let bridge;
    Object.defineProperty(window, "RichBridge", {
      configurable: true,
      get: () => bridge,
      set(value) {
        const invoke = value.invoke.bind(value);
        bridge = { ...value, async invoke(command, args) {
          if (command.startsWith("glance_") || command === "richfiles_settings" || command === "richfiles_open") {
            calls.push({ command, args });
          }
          switch (command) {
            case "glance_capture_status":
              if (delayTools) await new Promise(resolve => { window.releaseGlanceStatus = resolve; });
              return structuredClone(state.shortcut);
            case "glance_capture_configure":
              if (args.shortcut.code === "KeyX") throw new Error("That shortcut is unavailable.");
              Object.assign(state.shortcut, args, { registered: args.enabled });
              return structuredClone(state.shortcut);
            case "glance_workflow_status": return structuredClone(state.workflow);
            case "glance_workflow_configure":
              for (const [key, value] of Object.entries(args)) if (value !== null) state.workflow[key] = value;
              return structuredClone(state.workflow);
            case "richfiles_settings":
              if (args.option === "pictures") state.viewer.pictures = args.enabled;
              return structuredClone(state.viewer);
            case "glance_capture_open":
            case "richfiles_open": return null;
            default: return invoke(command, args);
          }
        } };
      },
    });
  }, { theme, available, delayTools });
  await page.clock.setFixedTime(new Date("2026-11-21T09:00:00Z"));
  await page.goto("file://" + path.join(UI_DIR, "index.html"));
  await leaveHome(page);
  await shellSettled(page);
  return page;
}

async function checkContrast(page, container) {
  await page.addScriptTag({ content: contrast.pageScript() });
  const facts = await page.locator(container).evaluate(root => {
    const C = window.__contrastMath;
    const parse = value => C.parseCssColor(value) ?? (m => m && {
      r: 255 * Number(m[1]), g: 255 * Number(m[2]), b: 255 * Number(m[3]), a: Number(m[4] ?? 1),
    })(/^color\(srgb ([\d.e-]+) ([\d.e-]+) ([\d.e-]+)(?: \/ ([\d.e-]+))?\)$/.exec(value));
    function ground(node) {
      if (!node) return { r: 0, g: 0, b: 0, a: 0 };
      const fill = parse(getComputedStyle(node).backgroundColor);
      if (!fill) throw new Error("Unrecognized background " + getComputedStyle(node).backgroundColor);
      return fill.a === 1 ? fill : C.compositeOver(fill, ground(node.parentElement));
    }
    const failures = [];
    let minimumText = Infinity, minimumControl = Infinity, textCount = 0, controlCount = 0;
    function check(paint, bg, threshold, label) {
      const fill = parse(paint);
      if (!fill) throw new Error("Unrecognized paint " + paint);
      const ratio = C.contrastRatio(C.compositeOver(fill, bg), bg);
      if (threshold === 3) {
        minimumControl = Math.min(ratio, minimumControl);
        controlCount++;
      } else {
        minimumText = Math.min(ratio, minimumText);
        textCount++;
      }
      if (ratio < threshold) failures.push({ label, ratio: C.round2(ratio), threshold });
    }
    for (const node of root.querySelectorAll("*")) {
      if (!node.getClientRects().length || node.closest("[hidden]") || getComputedStyle(node).visibility !== "visible") continue;
      const style = getComputedStyle(node);
      const text = [...node.childNodes].find(child => child.nodeType === Node.TEXT_NODE && child.textContent.trim());
      if (text) {
        const size = parseFloat(style.fontSize);
        if (size < 16) failures.push({ label: text.textContent.trim(), size });
        // The only large text on these pages is the title.
        check(style.color, ground(node), size >= 24 ? 3 : 4.5, text.textContent.trim());
      }
      const label = node.id || node.className;
      if (node.matches('[role="switch"]')) {
        check(style.backgroundColor, ground(node.parentElement), 3, label + " track");
        check(getComputedStyle(node, "::after").backgroundColor, ground(node), 3, label + " knob");
      }
      if (node.matches("button:not([role=switch]), kbd.gp-key")) {
        check(style.borderTopColor, ground(node.parentElement), 3, label + " edge");
      }
      if (node.matches(".bp-dot, #nr-brief-dot")) {
        check(style.backgroundColor, ground(node.parentElement), 3, label);
      }
      if (node.matches(".bp-fi svg, .gp-fi svg, .bp-state > svg, .gp-seg svg, svg.nr-glyph")) {
        check(style.color, ground(node), 3, String(label));
      }
    }
    return { failures, minimumText, minimumControl, textCount, controlCount };
  });
  assertEqual(facts.failures, [], container + " WCAG AA");
  assert(facts.textCount >= 10 && facts.controlCount >= 5, "contrast inventory must include visible text and controls");
  return `text ${facts.minimumText.toFixed(2)}:1, controls ${facts.minimumControl.toFixed(2)}:1`;
}

async function main() {
  const run = createRun("RichBrief and RichGlance rail pages (rounds 98 and 99)");
  const browser = await loadPlaywright().webkit.launch();

  for (const theme of ["light", "dark"]) {
    await run.check(theme + ": rail order, page artwork and WCAG AA in the resting and live states", async () => {
      const page = await open(browser, theme);
      try {
        assertEqual(await page.locator("#nav-rail .nr-group [data-item]").evaluateAll(items =>
          items.map(node => node.dataset.item).slice(0, 9)),
        ["home", "chats", "finder", "richfiles", "glance", "cleaner", "scribe", "brief", "pass"], "rail order");
        const records = [];
        for (const [name, section] of [["brief", "brief-page"], ["glance", "glance-page"]]) {
          await page.click("#nr-" + name);
          await page.locator("#" + section).waitFor();
          assertEqual(await page.getAttribute("#nr-" + name, "aria-current"), "page");
          const primary = page.locator(name === "brief" ? "#bp-start" : "#gp-capture");
          await primary.hover();
          assertEqual(await primary.evaluate(node => getComputedStyle(node).filter), "none",
            "the gold hover keeps the measured fill and edge colors");
          records.push(section + " at rest " + await checkContrast(page, "#" + section));
          await page.mouse.move(370, 65);
          if (name === "brief") {
            await page.evaluate(() => window.__RICHOS_MOCK__.meetingSet({
              recording: true, startedAt: Date.now() - 42000,
              calendarRecovery: [{ vendor: "google", account: "calendar@example.com" }],
            }));
          } else {
            await page.locator("#gp-shortcut:enabled").waitFor();
            await page.click("#gp-shortcut");
            await page.locator("#gp-shortcut[aria-checked=true]").waitFor();
            await page.click("#gp-history");
            await page.locator("#gp-history[aria-checked=true]").waitFor();
            await page.click("#gp-history");
            await page.locator("#gp-history-confirm").waitFor();
          }
          records.push(section + " " + await checkContrast(page, "#" + section));
          const shot = await captureSettled(page, { animations: "disabled" });
          publishShot(shot, path.join(SHOT_DIR, theme + "-" + name + ".png"));
        }
        return records.join(", ");
      } finally {
        await page.close();
      }
    });
  }

  await run.check("late rail items move the selection bar without another press", async () => {
    const page = await open(browser, "dark", true, true);
    try {
      await page.click("#nr-pass");
      const centers = async () => {
        await flushFrames(page);
        return page.locator("#nav-rail").evaluate(nav => {
          const y = element => {
            const rect = element.getBoundingClientRect();
            return rect.top + rect.height / 2;
          };
          return { selected: y(nav.querySelector("[aria-current=page]")), bar: y(nav.querySelector(".nr-ind")) };
        });
      };
      const first = await centers();
      assert(Math.abs(first.selected - first.bar) <= 1, "the bar starts beside RichPass");
      await page.evaluate(() => window.releaseGlanceStatus());
      await page.locator("#nr-glance").waitFor();
      const withGlance = await centers();
      assert(withGlance.selected > first.selected, "RichGlance moves RichPass down");
      assert(Math.abs(withGlance.selected - withGlance.bar) <= 1, "the bar follows when RichGlance appears");
      await page.evaluate(() => window.__RICHOS_MOCK__.meetingSet({ available: true }));
      await page.locator("#nr-brief").waitFor();
      const withBrief = await centers();
      assert(withBrief.selected > withGlance.selected, "RichBrief moves RichPass down");
      assert(Math.abs(withBrief.selected - withBrief.bar) <= 1, "the bar follows when RichBrief appears");
      await page.evaluate(() => window.__RICHOS_MOCK__.meetingSet({ available: false }));
      await page.locator("#nr-brief").waitFor({ state: "hidden" });
      const withoutBrief = await centers();
      assertEqual(withoutBrief.selected, withGlance.selected);
      assert(Math.abs(withoutBrief.selected - withoutBrief.bar) <= 1, "the bar follows when RichBrief disappears");
    } finally {
      await page.close();
    }
  });

  await run.check("RichGlance controls use the native capture, shortcut, workflow and image viewer commands", async () => {
    const page = await open(browser, "light");
    try {
      assertEqual(await page.locator("#nr-glance").count(), 1);
      await page.click("#nr-glance");
      for (const id of ["gp-shortcut", "gp-history", "gp-viewer"]) {
        await page.locator("#" + id + "[aria-checked=false]:enabled").waitFor();
      }
      await page.click("#gp-capture");
      await page.click("#gp-change");
      await page.keyboard.press("Shift+Digit7");
      await page.locator("#gp-key-error").waitFor();
      assertEqual(await page.locator("#gp-key-error").innerText(), "Use two or more modifiers and a letter or number.");
      await page.keyboard.press("Escape");
      assertEqual(await page.locator("#gp-keys").textContent(), "⌃⇧C");
      await page.click("#gp-change");
      await page.keyboard.press("Control+Shift+KeyX");
      await page.locator("#gp-key-error").waitFor();
      assert((await page.locator("#gp-key-error").innerText()).includes("That shortcut is unavailable."), "collision leaves the picker available");
      await page.locator("#gp-change[aria-label^=Cancel]:enabled").waitFor();
      await page.keyboard.press("Control+Shift+Digit7");
      await page.locator("#gp-keys").getByText("7", { exact: true }).waitFor();
      await page.click("#gp-shortcut");
      await page.locator("#gp-cap kbd").waitFor();
      assertEqual(await page.locator("#gp-cap kbd").textContent(), "⌃⇧7");

      await page.click("#gp-after-edit");
      await page.locator("#gp-after-edit[aria-pressed=true]:enabled").waitFor();
      await page.click("#gp-after-copy");
      await page.locator("#gp-after-copy[aria-pressed=true]:enabled").waitFor();
      await page.click("#gp-history");
      await page.locator("#gp-history[aria-checked=true]:enabled").waitFor();
      await page.click("#gp-history");
      await page.click("#gp-history-keep");
      assert(await page.locator("#gp-history-confirm").isHidden(), "cancel dismisses the confirmation");
      assertEqual(await page.getAttribute("#gp-history", "aria-checked"), "true");
      await page.click("#gp-history");
      await page.click("#gp-history-remove");
      await page.locator("#gp-history[aria-checked=false]:enabled").waitFor();

      await page.click("#gp-viewer");
      await page.locator("#gp-viewer[aria-checked=true]:enabled").waitFor();
      await page.click("#gp-richfiles");
      const calls = await page.evaluate(() => window.glanceCalls);
      assertEqual(calls.filter(call => call.command === "glance_capture_open").length, 1);
      assertEqual(calls.filter(call => call.command === "glance_workflow_configure").map(call => call.args), [
        { opensEditor: true, keepsHistory: null }, { opensEditor: false, keepsHistory: null },
        { opensEditor: null, keepsHistory: true }, { opensEditor: null, keepsHistory: false },
      ]);
      assert(calls.some(call => call.command === "richfiles_settings" && call.args.option === "pictures" && call.args.enabled), "viewer uses the shared RichFiles setting");
      assert(calls.some(call => call.command === "richfiles_open" && call.args.imageViewerSettings), "opens the image viewer settings");
      await page.click("#set-btn");
      await page.click("#set-richglance-capture-open");
      await page.locator("#richglance-capture-keys").getByText("7", { exact: true }).waitFor();
      assertEqual(await page.getAttribute("#richglance-capture-on", "aria-checked"), "true");
    } finally {
      await page.close();
    }
  });

  await run.check("the RichGlance icon is absent when RichGlance is unavailable", async () => {
    const page = await open(browser, "dark", false);
    try {
      assertEqual(await page.locator("#nr-glance[hidden]").count(), 1);
    } finally {
      await page.close();
    }
  });

  await browser.close();
  process.exitCode = run.report() ? 1 : 0;
}

main().catch(error => { console.error(error); process.exitCode = 1; });

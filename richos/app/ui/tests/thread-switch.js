"use strict";

const path = require("path");
const { spawnSync } = require("child_process");
const { loadPlaywright, leaveHome, createRun, assert, assertEqual, UI_DIR } = require("./lib/harness");
const APP = "file://" + path.join(UI_DIR, "index.html");

async function open(page, id) {
  await page.locator(`.nav-thread[data-thread-id="${id}"]`).click();
  await page.waitForFunction(() => document.querySelector("#composer-row").dataset.mode !== "opening");
}

async function holdActivation(page) {
  await page.evaluate(() => {
    const invoke = window.RichBridge.invoke.bind(window.RichBridge);
    window.activationGates = [];
    window.sentBindings = [];
    window.RichBridge.invoke = async (cmd, args) => {
      if (cmd === "switch_thread") await new Promise((resolve, reject) => {
        window.activationGates.push({ resolve, reject, thread: args.threadId });
      });
      if (cmd === "send_message") window.sentBindings.push(args);
      return invoke(cmd, args);
    };
  });
}

async function clickAndPaint(page, id) {
  return page.evaluate(async (thread) => {
    document.querySelector(`.nav-thread[data-thread-id="${thread}"]`).click();
    await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    const pane = document.querySelector("#conversation");
    return {
      entity: document.querySelector("#scope-entity").textContent,
      title: document.querySelector("#scope-thread").textContent,
      text: pane.innerText,
      visible: !pane.hidden && pane.getBoundingClientRect().height > 0,
      opening: document.querySelector("#composer-row").dataset.mode === "opening",
    };
  }, id);
}

async function release(page, fail = false) {
  await page.waitForFunction(() => window.activationGates.length > 0);
  await page.evaluate(fail => {
    const gate = window.activationGates.shift();
    if (fail) gate.reject(new Error("The conversation could not be opened."));
    else gate.resolve();
  }, fail);
}

async function main() {
  const run = createRun("Thread display while activation is held");
  const browser = await loadPlaywright().webkit.launch();
  const page = await browser.newPage({ viewport: { width: 1400, height: 900 } });
  const errors = [];
  page.on("pageerror", error => errors.push(String(error)));
  try {
    await page.goto(APP);
    await leaveHome(page);
    await open(page, "hiring");
    const saved = "The hiring cache marker";
    await page.evaluate(text => window.__RICHOS_MOCK__.simulateSlowTurn("hiring", text, 80), saved);
    await page.waitForFunction(text => document.querySelector("#messages").innerText.includes(text), saved);
    const title = await page.locator("#scope-thread").innerText();
    await open(page, "acme");
    await holdActivation(page);

    await run.check("cached destination paints before backend activation", async () => {
      const view = await clickAndPaint(page, "hiring");
      assert(view.opening, "activation must still be held");
      assert(view.visible, "the conversation pane must be visible");
      assertEqual(view.title, title, "the breadcrumb names the selected destination");
      assert(view.text.includes(saved.trim()), "the saved destination messages remain visible");
      assert(!(await page.locator("body").innerText()).includes("Waiting for its saved messages"), "cached messages need no loading placeholder, including the wait band");
    });
    await release(page);
    await page.waitForFunction(() => document.querySelector("#composer-row").dataset.mode !== "opening");

    await run.check("a cache miss paints the destination and a loading state", async () => {
      const view = await clickAndPaint(page, "partner");
      assert(view.opening && view.visible, "cold destination is visible during activation");
      assert(view.title && view.title !== title, "the previous breadcrumb is gone");
      assert(view.text.includes("Waiting for its saved messages"), "the cache miss is explicit");
      assert(!view.text.includes(saved.trim()), "another entity's content cannot carry across");
      const ratios = [];
      for (const theme of ["light", "dark"]) {
        const colors = await page.evaluate(async theme => {
          document.documentElement.setAttribute("data-theme", theme);
          await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
          const text = document.querySelector(".thread-loading");
          let background = text;
          while (background && ["rgba(0, 0, 0, 0)", "transparent"].includes(getComputedStyle(background).backgroundColor)) {
            background = background.parentElement;
          }
          return [getComputedStyle(text).color, getComputedStyle(background || document.body).backgroundColor];
        }, theme);
        const contrast = spawnSync("python3", [path.resolve(UI_DIR, "../scripts/qa/contrast.py"), ...colors], { encoding: "utf8" });
        assertEqual(contrast.status, 0, theme + ": " + contrast.stdout + contrast.stderr);
        ratios.push(theme + ": " + contrast.stdout.trim());
      }
      return ratios.join("; ");
    });

    await run.check("rapid navigation cannot let an older activation repaint the latest selection", async () => {
      const latest = await clickAndPaint(page, "hiring");
      await release(page); // partner
      await page.waitForFunction(() => window.activationGates.length > 0);
      assertEqual(await page.locator("#scope-thread").innerText(), latest.title, "late partner activation cannot paint");
      assert((await page.locator("#conversation").innerText()).includes(saved.trim()), "cached hiring remains displayed");
      await release(page); // hiring
      await page.waitForFunction(() => document.querySelector("#composer-row").dataset.mode !== "opening");
      const binding = await page.evaluate(() => window.RichBridge.invoke("active_context"));
      assertEqual(binding.thread_id, "hiring", "serialized activation ends on the last choice");
    });

    await run.check("a send reaches the selected destination before activation completes", async () => {
      const view = await clickAndPaint(page, "acme");
      try {
        assert(view.opening, "the destination activation is held");
        await page.locator("#input").fill("Queued for Acme only");
        await page.locator("#input").press("Enter");
        assertEqual(await page.evaluate(() => window.sentBindings.length), 1, "sending does not wait for activation");
        const args = await page.evaluate(() => window.sentBindings[0]);
        assertEqual(args.threadId, "acme", "the send carries the selected destination thread");
        assertEqual(args.text, "Queued for Acme only", "the original message reaches that destination");
      } finally {
        await release(page);
        await page.waitForFunction(() => document.querySelector("#composer-row").dataset.mode !== "opening");
      }
      assertEqual(await page.evaluate(() => window.sentBindings.length), 1, "activation does not send the message again");
    });

    await run.check("activation failure does not expose cached content as an active conversation", async () => {
      await clickAndPaint(page, "hiring");
      await release(page, true);
      await page.waitForSelector("#unbound-view:not([hidden])");
      assert(await page.locator("#conversation").isHidden(), "failed activation hides the cached pane");
      assertEqual(await page.evaluate(() => window.sentBindings.length), 1, "no send occurred against failed activation");
    });
    await run.check("a pin that could not be saved is said in the rail, in both themes at 16px or more", async () => {
      const text = "the navigation file could not be read, so it was left untouched and this change was not saved";
      await page.evaluate(msg => {
        const invoke = window.RichBridge.invoke.bind(window.RichBridge);
        window.RichBridge.invoke = async (cmd, args) => {
          if (cmd === "set_thread_pinned") throw msg;
          return invoke(cmd, args);
        };
      }, text);
      assert(await page.locator("#nav-save-note").isHidden(), "no note before a failure");
      await page.hover('.nav-thread[data-thread-id="acme"]');
      await page.click('.nav-thread[data-thread-id="acme"] + .nav-thread-more');
      await page.locator("#thread-menu button", { hasText: /^Pin$/ }).click();
      await page.waitForSelector("#nav-save-note:not([hidden])");
      assertEqual(await page.locator("#nav-save-note").innerText(),
        "The navigation file could not be read, so it was left untouched and this change was not saved.");
      for (const theme of ["dark", "light"]) {
        const m = await page.evaluate(async t => {
          document.documentElement.dataset.theme = t;
          await new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)));
          await Promise.all(document.getAnimations().map(a => a.finished.catch(() => {})));
          const n = document.getElementById("nav-save-note");
          const cs = getComputedStyle(n);
          let rail = "rgba(0, 0, 0, 0)";
          for (let e = n; e && rail === "rgba(0, 0, 0, 0)"; e = e.parentElement) rail = getComputedStyle(e).backgroundColor;
          return { fg: cs.color, bg: rail, px: parseFloat(cs.fontSize) };
        }, theme);
        const lum = c => { const v = c.match(/[\d.]+/g).slice(0, 3).map(x => { x = x / 255; return x <= 0.03928 ? x / 12.92 : Math.pow((x + 0.055) / 1.055, 2.4); }); return 0.2126 * v[0] + 0.7152 * v[1] + 0.0722 * v[2]; };
        const a = lum(m.fg), b = lum(m.bg);
        const ratio = (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
        assert(ratio >= 4.5, theme + " contrast " + ratio.toFixed(2) + " " + m.fg + " on " + m.bg);
        assert(m.px >= 16, theme + " size " + m.px);
      }
    });
    // CEO, nightly 54 (2026-10-13, right-click-in-left-bar.mov): a right-click anywhere on a
    // thread row showed WebKit's own "Reload" menu, or selected the title and offered Look Up /
    // Translate. He expects the row's own menu, the one its ⋯ button opens.
    const rightClick = async (selector, position) => {
      await page.keyboard.press("Escape");
      await page.evaluate(() => {
        window.getSelection().removeAllRanges();
        window.__nativeMenuAllowed = null;
        window.addEventListener("contextmenu", e => { window.__nativeMenuAllowed = !e.defaultPrevented; }, { once: true });
      });
      const box = await page.locator(selector).boundingBox();
      const at = { x: box.x + position(box).x, y: box.y + position(box).y };
      await page.mouse.click(at.x, at.y, { button: "right" });
      await page.waitForTimeout(50);
      return page.evaluate(() => {
        const m = document.getElementById("thread-menu");
        const r = m.getBoundingClientRect();
        return {
          open: !m.hidden,
          items: [...m.querySelectorAll(".menu-item")].map(b => b.textContent),
          left: r.left, top: r.top,
          nativeMenuAllowed: window.__nativeMenuAllowed,
          selection: window.getSelection().toString(),
        };
      }).then(view => ({ ...view, at }));
    };
    const row = '.nav-thread-row:has(.nav-thread[data-thread-id="acme"])';
    for (const [where, position] of [
      ["on the title text", () => ({ x: 20, y: 0 }) ],
      ["on the empty space between the title and the ⋯", box => ({ x: box.width - 40, y: 0 })],
      ["on the ⋯ button itself", box => ({ x: box.width - 11, y: 0 })],
    ]) {
      await run.check(`a right-click ${where} opens the row's own menu, never WebKit's`, async () => {
        const view = await rightClick(row, box => ({ x: position(box).x, y: box.height / 2 }));
        assert(view.open, "the thread menu must open on a right-click");
        assertEqual(view.items.join(" | "), "Rename… | Pin | Archive", "the same menu the ⋯ button opens");
        assertEqual(view.nativeMenuAllowed, false, "the browser's own context menu must be suppressed");
        assertEqual(view.selection, "", "a right-click must not select the title text");
        assert(Math.abs(view.left - view.at.x) <= 2 && Math.abs(view.top - view.at.y) <= 2,
          `the menu opens at the pointer (${view.at.x},${view.at.y}), was at (${view.left},${view.top})`);
        return `menu at (${Math.round(view.left)},${Math.round(view.top)}) for a click at (${Math.round(view.at.x)},${Math.round(view.at.y)})`;
      });
    }
    await run.check("a right-click outside any thread row closes the open menu", async () => {
      await rightClick(row, () => ({ x: 20, y: 10 }));
      assert(await page.locator("#thread-menu").isVisible(), "precondition: the menu is open");
      // The middle of the main pane, well right of the rail (viewport 1400 x 900).
      await page.mouse.click(900, 500, { button: "right" });
      assert(await page.locator("#thread-menu").isHidden(), "the row menu must not stay open");
    });
    await run.check("no browser errors", () => assertEqual(errors.length, 0, errors.join("\n")));
  } finally {
    await browser.close();
  }
  process.exitCode = run.report() ? 1 : 0;
}
main().catch(error => { console.error(error); process.exitCode = 1; });

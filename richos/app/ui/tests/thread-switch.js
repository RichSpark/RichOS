"use strict";

const path = require("path");
const { spawnSync } = require("child_process");
const { loadPlaywright, leaveHome, createRun, assert, assertEqual, UI_DIR } = require("./lib/harness");
const contrast = require("./lib/contrast");
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
    // CEO, nightly 65 (2026-10-19): a right-click on the company name showed WebKit's Look Up /
    // Translate / Search with Google / Share / Speech / Services menu and selected part of the
    // name. no-native-menu.js removes it for the whole window; a text field keeps its editing menu.
    const probeContextClick = async (locator, fieldHint) => {
      await page.keyboard.press("Escape");
      await page.evaluate(() => {
        window.getSelection().removeAllRanges();
        window.__ctx = null;
        window.addEventListener("contextmenu", e => { window.__ctx = { prevented: e.defaultPrevented }; }, { once: true });
      });
      const box = await locator.boundingBox();
      await page.mouse.click(box.x + Math.min(box.width / 2, 20), box.y + box.height / 2, { button: "right" });
      await page.waitForTimeout(50);
      return page.evaluate(() => ({ ...window.__ctx, selection: window.getSelection().toString() }));
    };
    const labelOf = async (selector) => {
      const handle = await page.evaluateHandle(selector => {
        const walker = document.createTreeWalker(document.querySelector(selector), NodeFilter.SHOW_TEXT);
        for (let n = walker.nextNode(); n; n = walker.nextNode()) {
          const el = n.parentElement, r = el.getBoundingClientRect();
          if (n.textContent.trim().length > 3 && r.width > 0 && r.height > 0) return el;
        }
        return null;
      }, selector);
      return handle.asElement();
    };
    await run.check("the app's own menu still opens on a thread row", async () => {
      const view = await rightClick(row, box => ({ x: 20, y: box.height / 2 }));
      assert(view.open, "the thread menu must still open");
    });

    // CEO, 2026-10-19: "And of course, the user is free to manually change that thread title any
    // time they want." Round 35 (approved 2026-10-20): Rename… makes the row a field in place.
    // It used to call window.prompt, which the app's WebView answers with nothing (nightly 68).
    await page.goto(APP);
    await leaveHome(page);
    await open(page, "acme");
    await page.evaluate(() => {
      const invoke = window.RichBridge.invoke.bind(window.RichBridge);
      window.renames = [];
      window.RichBridge.invoke = async (cmd, args) => {
        if (cmd === "rename_thread") window.renames.push(args);
        return invoke(cmd, args);
      };
    });
    const acmeTitle = () => page.locator('.nav-thread[data-thread-id="acme"] .nav-thread-title').innerText();
    const field = page.locator(".nav-rename-input");
    const fieldView = () => page.evaluate(() => {
      const input = document.querySelector(".nav-rename-input");
      const note = document.querySelector(".nav-rename-note");
      if (!input) return null;
      const cs = getComputedStyle(input);
      return {
        focused: document.activeElement === input,
        value: input.value,
        selected: [input.selectionStart, input.selectionEnd],
        textLeft: input.getBoundingClientRect().left + parseFloat(cs.borderLeftWidth) + parseFloat(cs.paddingLeft),
        invalid: input.getAttribute("aria-invalid"),
        edge: cs.borderTopColor,
        danger: getComputedStyle(document.documentElement).getPropertyValue("--danger").trim(),
        note: note ? note.innerText.replace(/\s+/g, " ").trim() : null,
      };
    });
    const focusedRow = () => page.evaluate(() => {
      const a = document.activeElement;
      return a && a.classList.contains("nav-thread") ? a.dataset.threadId : (a ? a.tagName + "." + a.className : null);
    });
    const before = await acmeTitle();
    await run.check("Rename… from the ⋯ turns the row into a field in place, title selected, hint under it", async () => {
      const titleLeft = await page.evaluate(() =>
        document.querySelector('.nav-thread[data-thread-id="acme"] .nav-thread-title').getBoundingClientRect().left);
      await page.hover('.nav-thread[data-thread-id="acme"]');
      await page.click('.nav-thread[data-thread-id="acme"] + .nav-thread-more');
      await page.locator("#thread-menu button", { hasText: /^Rename…$/ }).click();
      await field.waitFor();
      const v = await fieldView();
      assert(v.focused, "the field has the keyboard");
      assertEqual(v.value, before, "the field starts with the chat's title");
      assertEqual(v.selected, [0, before.length], "the whole title is selected");
      assertEqual(v.note, "Enter to save · Esc to cancel", "the hint under the field");
      assert(Math.abs(v.textLeft - titleLeft) <= 1, `the first letter stays put: ${titleLeft} before, ${v.textLeft} in the field`);
      return `"${before}" selected; first letter at ${titleLeft.toFixed(1)} px before, ${v.textLeft.toFixed(1)} px in the field`;
    });
    await run.check("Enter saves through rename_thread: the row, the header and the store carry it, focus back on the row", async () => {
      await page.keyboard.type("Acme counteroffer");
      await page.keyboard.press("Enter");
      await page.waitForFunction(() => window.renames.length === 1);
      assertEqual(await page.evaluate(() => window.renames), [{ threadId: "acme", title: "Acme counteroffer" }]);
      await page.waitForFunction(() => document.querySelector('.nav-thread[data-thread-id="acme"] .nav-thread-title').textContent === "Acme counteroffer");
      assertEqual(await field.count(), 0, "the field is gone");
      assertEqual(await page.locator(".nav-rename-note").count(), 0, "the hint is gone");
      assertEqual(await page.locator("#scope-thread").innerText(), "Acme counteroffer", "the header above the chat changes with it");
      const stored = await page.evaluate(async () => {
        const tree = await window.RichBridge.invoke("navigation_tree");
        for (const g of tree.groups) for (const t of g.threads) if (t.id === "acme") return t.display_title;
        return null;
      });
      assertEqual(stored, "Acme counteroffer", "the store keeps the new title");
      assertEqual(await focusedRow(), "acme", "the keyboard is back on the row");
      assert(await page.locator('.nav-thread-row:has(.nav-thread[data-thread-id="acme"])').evaluate(n => n.classList.contains("is-renamed")),
        "the renamed row catches the gold light");
    });
    await run.check("Escape cancels: the old title comes back and nothing is saved (menu by Shift-F10)", async () => {
      await page.locator('.nav-thread[data-thread-id="acme"]').focus();
      await page.keyboard.press("Shift+F10");
      await page.waitForSelector("#thread-menu:not([hidden])");
      await page.keyboard.press("Enter"); // Rename… is the first item and has the keyboard
      await field.waitFor();
      await page.keyboard.type("Throwaway");
      await page.keyboard.press("Escape");
      assertEqual(await field.count(), 0, "the field is gone");
      assertEqual(await acmeTitle(), "Acme counteroffer", "the title is unchanged");
      assertEqual(await page.evaluate(() => window.renames.length), 1, "Escape saves nothing");
      assertEqual(await focusedRow(), "acme", "the keyboard is back on the row");
      assert(await page.locator("#thread-menu").isHidden(), "Escape in the field closes nothing else");
    });
    await run.check("an empty title is never saved: red edge and the refusal; typing clears it; clicking away restores", async () => {
      const box = await page.locator('.nav-thread-row:has(.nav-thread[data-thread-id="acme"])').boundingBox();
      await page.mouse.click(box.x + 30, box.y + box.height / 2, { button: "right" });
      await page.locator("#thread-menu button", { hasText: /^Rename…$/ }).click();
      await field.waitFor();
      await page.keyboard.press("Backspace");
      await page.keyboard.press("Enter");
      let v = await fieldView();
      assert(v, "the field stays open");
      assertEqual(v.invalid, "true", "the field is marked invalid");
      assertEqual(v.note, "A chat needs a name. Esc keeps the old one.", "the refusal under the field");
      const danger = await page.evaluate(c => { const s = document.createElement("span"); s.style.color = c; document.body.appendChild(s); const out = getComputedStyle(s).color; s.remove(); return out; }, v.danger);
      assertEqual(v.edge, danger, "the field's edge turns the danger color");
      assertEqual(await page.evaluate(() => window.renames.length), 1, "nothing was saved");
      await page.keyboard.type("x");
      v = await fieldView();
      assertEqual(v.invalid, null, "typing clears the refusal");
      assertEqual(v.note, "Enter to save · Esc to cancel", "the hint is back");
      await page.keyboard.press("Backspace");
      await page.mouse.click(900, 500);
      assertEqual(await field.count(), 0, "clicking away closes the field");
      assertEqual(await acmeTitle(), "Acme counteroffer", "clicking away from an empty field puts the old title back");
      assertEqual(await page.evaluate(() => window.renames.length), 1, "nothing was saved");
    });
    await run.check("the hint and the refusal are 16px and pass 4.5:1 in both themes", async () => {
      const ratios = [];
      await page.hover('.nav-thread[data-thread-id="acme"]');
      await page.click('.nav-thread[data-thread-id="acme"] + .nav-thread-more');
      await page.locator("#thread-menu button", { hasText: /^Rename…$/ }).click();
      await field.waitFor();
      for (const state of ["hint", "refusal"]) {
        if (state === "refusal") { await page.keyboard.press("Backspace"); await page.keyboard.press("Enter"); }
        for (const theme of ["dark", "light"]) {
          // The app's own checker (lib/contrast.js): it composites the translucent --ink-soft and
          // the translucent dark rail over what is really painted behind them.
          const probe = await page.evaluate(async ([t, script]) => {
            document.documentElement.dataset.theme = t;
            await new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)));
            if (!window.__contrastProbe) (0, eval)(script);
            const out = window.__contrastProbe({ surface: "rename", theme: t });
            const ours = (bucket) => Object.entries(bucket).filter(([k, v]) => /rename-note|nav-rename/.test(k + " " + (v.selector || v.path || "")));
            const sizes = [...document.querySelectorAll(".nav-rename-note, .nav-rename-note *")].map(n => parseFloat(getComputedStyle(n).fontSize));
            return {
              measured: out.measuredPaths.filter(p => /rename-note/.test(p)).length,
              failures: ours(out.failures), unresolvable: ours(out.unresolvable), sizes,
            };
          }, [theme, contrast.pageScript()]);
          assert(probe.measured > 0, `${state} ${theme}: the checker measured none of the note's text`);
          assertEqual(probe.failures, [], `${state} ${theme}: below 4.5:1`);
          assertEqual(probe.unresolvable, [], `${state} ${theme}: unresolvable`);
          assert(probe.sizes.every(px => px >= 16), `${state} ${theme}: sizes ${probe.sizes}`);
          ratios.push(`${state} ${theme}: ${probe.measured} text node(s) measured, 0 below 4.5:1`);
        }
      }
      await page.keyboard.press("Escape");
      return ratios.join("; ");
    });
    await run.check("clicking away from a changed title saves it", async () => {
      await page.hover('.nav-thread[data-thread-id="acme"]');
      await page.click('.nav-thread[data-thread-id="acme"] + .nav-thread-more');
      await page.locator("#thread-menu button", { hasText: /^Rename…$/ }).click();
      await field.waitFor();
      await page.keyboard.type("Acme deal, closed");
      await page.mouse.click(900, 500);
      await page.waitForFunction(() => window.renames.length === 2);
      assertEqual(await page.evaluate(() => window.renames[1]), { threadId: "acme", title: "Acme deal, closed" });
      await page.waitForFunction(() => document.querySelector("#scope-thread").textContent === "Acme deal, closed");
    });
    // Handover review rv-20261020T104343Z-914364e4-3d43: with the company overview open, a rename
    // from the sidebar left the overview's row on the old title, and after a routine navigation
    // refresh the overview's own Rename… offered the old title too.
    await run.check("renamed from the sidebar with the company overview open: the overview's row shows it and its Rename… offers it", async () => {
      await page.locator('.nav-group:has(.nav-thread[data-thread-id="acme"]) .nav-group-label').click();
      const inOverview = '#entity-view-thread-list .nav-thread[data-thread-id="acme"]';
      await page.waitForSelector(inOverview);
      assertEqual(await page.locator(inOverview + " .nav-thread-title").innerText(), "Acme deal, closed", "precondition");
      // A routine refresh (a turn event does this) replaces the rows the overview was built from.
      await page.evaluate(() => refreshNavigation());
      const inRail = '#rail-nav .nav-thread[data-thread-id="acme"]';
      await page.hover(inRail);
      await page.click(inRail + " + .nav-thread-more");
      await page.locator("#thread-menu button", { hasText: /^Rename…$/ }).click();
      await field.waitFor();
      await page.evaluate(sel => { window.__railRowBeforeSave = document.querySelector(sel); }, inRail);
      await page.keyboard.type("Acme counteroffer, signed");
      await page.keyboard.press("Enter");
      await page.waitForFunction(() => window.renames.length === 3);
      // The save is over once the rail has been rebuilt from the store (a new row element).
      await page.waitForFunction(sel => {
        const n = document.querySelector(sel);
        return n && n !== window.__railRowBeforeSave && n.textContent.includes("Acme counteroffer, signed");
      }, inRail);
      const overviewTitle = await page.locator(inOverview + " .nav-thread-title").innerText();
      assertEqual(overviewTitle, "Acme counteroffer, signed", "the overview's row carries the new title");
      await page.hover(inOverview);
      await page.click(inOverview + " + .nav-thread-more");
      await page.locator("#thread-menu button", { hasText: /^Rename…$/ }).click();
      await field.waitFor();
      const offered = (await fieldView()).value;
      await page.keyboard.press("Escape");
      assertEqual(offered, "Acme counteroffer, signed", "Rename… from the overview offers the current title");
      assertEqual(await page.evaluate(() => window.renames.length), 3, "Escape saved nothing");
      return `overview row: "${overviewTitle}"; its Rename… offered "${offered}"`;
    });
    await page.goto(APP);
    await leaveHome(page);
    await open(page, "acme");
    await page.locator("#input").fill("A message to right-click");
    await page.locator("#input").press("Enter");
    await page.waitForFunction(() => document.querySelector("#conversation").innerText.includes("A message to right-click"));
    for (const [what, selector] of [
      ["the company name in the sidebar", ".nav-group-label"],
      ["a message in the conversation", "#conversation"],
      ["the chrome above the conversation", "#scope-entity"],
    ]) {
      await run.check(`a right-click on ${what} shows no native menu and selects nothing`, async () => {
        const el = selector === ".nav-group-label" || selector === "#scope-entity" ? await page.locator(selector).first().elementHandle() : await labelOf(selector);
        assert(el, `${what} must be on screen`);
        const seen = await probeContextClick(el, false);
        assertEqual(seen.prevented, true, "the browser's own context menu must be suppressed");
        assertEqual(seen.selection, "", "a right-click must not select the text");
        return `on "${(await el.innerText()).trim().slice(0, 40)}"`;
      });
    }
    // Last on purpose: in a text field the native editing menu really opens, and in headless
    // WebKit an open native menu holds the pointer, so nothing after it would receive a click.
    await run.check("a right-click in the composer keeps the editing menu (Cut, Copy, Paste)", async () => {
      const seen = await probeContextClick(page.locator("#input"), true);
      assertEqual(seen.prevented, false, "the text field's own menu must stay");
    });
    await run.check("no browser errors", () => assertEqual(errors.length, 0, errors.join("\n")));
  } finally {
    await browser.close();
  }
  process.exitCode = run.report() ? 1 : 0;
}
main().catch(error => { console.error(error); process.exitCode = 1; });

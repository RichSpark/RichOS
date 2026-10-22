// THE QUESTION ABOUT THE USER'S CHANGED INSTRUCTIONS AND THE BEFORE AND AFTER, against the REAL
// screens under WebKit.
//
// The CEO, 2026-10-22: when a new version ships improved instructions and the user changed
// their `rich.md`/`stu.md`, the app asks once, in his words; on OK an editor rewrites the file,
// "and when done, the user is presented with "before" and "after" with all the edits
// highlighted". Built as drawn from richos-hq design/mockups/rounds/round-37/. The shell decides
// whether to ask and does the update (`richos-core`'s `instructions_update.rs`, tested there);
// what is driven here is the screens:
//
//   1. It asks only when the shell says so, in his words, naming each file and where it is.
//   2. No (and Escape) reaches `decline_instructions_update` and closes.
//   3. OK updates the files one after another, each with its state, then opens the before and
//      after: one tab per file with its count of edits, the edits marked, the user's own lines
//      edited out tagged "your line", stepping through the edits, the file opened or shown.
//   4. A refused update is said, with a way out.
//   5. Contrast, computed in BOTH themes off WebKit's resolved colors.
"use strict";

const path = require("path");
const { loadPlaywright, createRun, assert, assertEqual, UI_DIR } = require("./lib/harness");

const QUESTION =
  "You previously modified Rich’s instructions (rich.md and/or stu.md), but the new RichOS " +
  "version has improved/updated instructions now. OK to update your modified instructions too?";
const ASKED = [{ name: "rich.md", path: "~/myrichos/rich.md" }, { name: "stu.md", path: "~/myrichos/stu.md" }];
/// What the shell answers for each file (`instructions_update::UpdatedFile`).
const UPDATED = {
  "rich.md": {
    name: "rich.md", path: "~/myrichos/rich.md", edits: 3, said: "Done.",
    rows: [
      [["fm", "---\nname: rich\n---"]],
      [["s", "You are Rich."]],
      [["s", "Hand work over when it is long. "], ["y", "Hand it to Stu even when I ask you to do it yourself."],
       ["a", "When the user asks you to do something yourself, it is yours."]],
      [["s", "Use `send` for context"], ["d", " and corrections."], ["a", ", corrections and ordinary communication."]],
      [["a", "Check the result, then say it."]],
      [["s", "Call me Dana."]],
    ],
  },
  "stu.md": {
    name: "stu.md", path: "~/myrichos/stu.md", edits: 1, said: "Done.",
    rows: [[["s", "You are Stu."]], [["a", "RichOS moves you to another account when one is nearly used up."]]],
  },
};

function body(theme) {
  // The rail wordmark's two paths, which the question's mark is drawn from.
  return `<!DOCTYPE html>
<html lang="en" data-theme="${theme}"><head><meta charset="utf-8"></head><body>
<svg id="rail-wordmark" viewBox="0 0 744 744"><path id="R" d="M0 0h10v10z"/><path id="arrow" d="M0 0h5v5z"/></svg>
<button id="before">before</button></body></html>`;
}

function bridge(asked, fail) {
  return `
    window.__invoked = [];
    window.__release = {};
    window.RichBridge = {
      invoke: (name, args) => {
        window.__invoked.push({name, args});
        if (name === "instructions_update_question") return Promise.resolve(${JSON.stringify(asked)});
        if (name === "update_instructions_file") {
          if (${fail}) return Promise.reject("the update did not write rich.md; nothing was changed");
          return new Promise((resolve) => { window.__release[args.name] = () => resolve(${JSON.stringify(UPDATED)}[args.name]); });
        }
        return Promise.resolve(null);
      },
    };
  `;
}

function ratioHelper() {
  return `
    window.__ratio = (node) => {
      if (typeof node === "string") node = document.querySelector(node);
      if (!node) throw new Error("no node");
      const parse = (value) => {
        const n = value.match(/[\\d.]+/g).map(Number);
        return { r: n[0], g: n[1], b: n[2], a: n.length > 3 ? n[3] : 1 };
      };
      // Composite every translucent background up to the first opaque one.
      const layers = [];
      for (let el = node; el; el = el.parentElement) {
        const c = parse(getComputedStyle(el).backgroundColor);
        if (c.a > 0) layers.push(c);
        if (c.a === 1) break;
      }
      let bg = { r: 255, g: 255, b: 255 };
      for (const c of layers.reverse()) bg = { r: c.r * c.a + bg.r * (1 - c.a), g: c.g * c.a + bg.g * (1 - c.a), b: c.b * c.a + bg.b * (1 - c.a) };
      const fg = parse(getComputedStyle(node).color);
      const flat = { r: fg.r * fg.a + bg.r * (1 - fg.a), g: fg.g * fg.a + bg.g * (1 - fg.a), b: fg.b * fg.a + bg.b * (1 - fg.a) };
      const channel = (v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4); };
      const lum = (c) => 0.2126 * channel(c.r) + 0.7152 * channel(c.g) + 0.0722 * channel(c.b);
      const a = lum(flat), b = lum(bg);
      return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
    };
    window.__fontPx = (node) => parseFloat(getComputedStyle(typeof node === "string" ? document.querySelector(node) : node).fontSize);
  `;
}

async function openPage(browser, theme, { asked = ASKED, fail = false } = {}) {
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  await page.setContent(body(theme));
  await page.addStyleTag({ path: path.join(UI_DIR, "style.css") });
  await page.addStyleTag({ path: path.join(UI_DIR, "instructions-update.css") });
  await page.focus("#before");
  await page.addScriptTag({ content: bridge(asked, fail) });
  await page.addScriptTag({ path: path.join(UI_DIR, "instructions-update.js") });
  await page.addScriptTag({ content: ratioHelper() });
  const styled = await page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue("--card").trim());
  assert(styled.length > 0, "the shipping stylesheet did not load; contrast here would be the browser's");
  page.__errors = errors;
  return page;
}

const names = (page) => page.evaluate(() => window.__invoked.map((i) => i.name));

async function toResult(page) {
  await page.locator("#instructions-update").waitFor({ state: "visible" });
  await page.locator("#iu-ok").click();
  await page.waitForFunction(() => typeof window.__release["rich.md"] === "function");
  await page.evaluate(() => window.__release["rich.md"]());
  await page.waitForFunction(() => typeof window.__release["stu.md"] === "function");
  await page.evaluate(() => window.__release["stu.md"]());
  await page.locator("#instructions-update-result").waitFor({ state: "visible" });
}

async function main() {
  const run = createRun("the question about the user's changed instructions, the update, and the before and after");
  const browser = await loadPlaywright().webkit.launch();

  await run.check("nothing is shown when the shell has nothing to ask", async () => {
    const page = await openPage(browser, "dark", { asked: [] });
    await page.waitForFunction(() => window.__invoked.length === 1);
    assertEqual(JSON.stringify(await names(page)), JSON.stringify(["instructions_update_question"]));
    assertEqual(await page.locator("#instructions-update").count(), 0, "the sheet was built for a user with nothing to ask");
    await page.close();
    return "asked the shell once, built nothing";
  });

  await run.check("the question is his words, names each file and where it is, and OK has the focus", async () => {
    const page = await openPage(browser, "dark");
    await page.locator("#instructions-update").waitFor({ state: "visible" });
    assertEqual(await page.locator("#iu-q-text").innerText(), QUESTION, "the question is not in his words");
    assertEqual(JSON.stringify(await page.locator("#iu-q-files code").allInnerTexts()), JSON.stringify(["~/myrichos/rich.md", "~/myrichos/stu.md"]));
    assertEqual(await page.locator("#iu-ok").innerText(), "OK");
    assertEqual(await page.locator("#iu-no").innerText(), "No");
    assertEqual(await page.evaluate(() => document.activeElement.id), "iu-ok", "OK must answer Return");
    assertEqual(await page.locator(".iu-q-mark svg path").count(), 2, "the RichOS mark is not drawn");
    assertEqual(page.__errors.length, 0, page.__errors.join("; "));
    await page.close();
    return "his sentence word for word, both files with their paths, OK focused";
  });

  await run.check("No and Escape reach decline_instructions_update, close, and give the focus back", async () => {
    for (const how of ["click", "Escape"]) {
      const page = await openPage(browser, "dark");
      await page.locator("#instructions-update").waitFor({ state: "visible" });
      if (how === "click") await page.locator("#iu-no").click(); else await page.keyboard.press("Escape");
      await page.locator("#instructions-update").waitFor({ state: "hidden" });
      assertEqual(JSON.stringify(await names(page)), JSON.stringify(["instructions_update_question", "decline_instructions_update"]), how);
      assertEqual(await page.evaluate(() => document.activeElement.id), "before", `${how}: the focus did not come back`);
      await page.close();
    }
    return "both answer No, which changes nothing";
  });

  await run.check("OK updates the files one after another, each with its state", async () => {
    const page = await openPage(browser, "dark");
    await page.locator("#instructions-update").waitFor({ state: "visible" });
    await page.locator("#iu-ok").click();
    await page.waitForFunction(() => typeof window.__release["rich.md"] === "function");
    assertEqual(await page.locator("#iu-work-title").innerText(), "Updating your instructions…");
    assert((await page.locator(".iu-q-lede").innerText()).includes("editing out your instructions that conflict with it"));
    const states = async () => JSON.stringify(await page.locator("#iu-w-files li").evaluateAll((ls) => ls.map((l) => [l.dataset.k, l.dataset.s, l.innerText.trim()])));
    assertEqual(await states(), JSON.stringify([["rich.md", "working", "~/myrichos/rich.md\nUpdating"], ["stu.md", "waiting", "~/myrichos/stu.md\nNext"]]));
    await page.evaluate(() => window.__release["rich.md"]());
    await page.waitForFunction(() => typeof window.__release["stu.md"] === "function");
    assertEqual(await states(), JSON.stringify([["rich.md", "done", "~/myrichos/rich.md\nDone"], ["stu.md", "working", "~/myrichos/stu.md\nUpdating"]]));
    await page.evaluate(() => window.__release["stu.md"]());
    await page.locator("#instructions-update-result").waitFor({ state: "visible" });
    const calls = await page.evaluate(() => window.__invoked.filter((i) => i.name === "update_instructions_file").map((i) => i.args.name));
    assertEqual(JSON.stringify(calls), JSON.stringify(["rich.md", "stu.md"]), "each file is updated once, in order");
    await page.close();
    return "rich.md then stu.md, Updating / Next / Done, then the before and after";
  });

  await run.check("the before and after marks every edit and tags the user's own lines", async () => {
    const page = await openPage(browser, "dark");
    await toResult(page);
    const tabs = await page.locator(".iu-tab").evaluateAll((ts) => ts.map((t) => [t.querySelector("code").textContent, t.querySelector(".iu-tab-n").textContent]));
    assertEqual(JSON.stringify(tabs), JSON.stringify([["rich.md", "3 edits"], ["stu.md", "1 edit"]]));
    assertEqual(await page.locator("#iu-path").innerText(), "~/myrichos/rich.md");
    // The user's own line, edited out and tagged; older RichOS wording, edited out without a tag.
    const before = await page.locator(".iu-before").allInnerTexts();
    assert(before[2].includes("your line") && before[2].includes("Hand it to Stu even when I ask you"), before[2]);
    assert(!before[3].includes("your line") && before[3].includes(" and corrections."), before[3]);
    assertEqual(await page.locator(".iu-yours").count(), 1, "only the user's own line carries the tag");
    assertEqual(await page.locator(".iu-del").count(), 2);
    assertEqual(await page.locator(".iu-ins").count(), 3);
    assertEqual(await page.locator(".iu-before.is-empty").count(), 1, "a paragraph new in the update has an empty space before");
    assertEqual(await page.locator(".iu-fm").first().innerText(), "---\nname: rich\n---");
    assert((await page.locator(".iu-after").nth(3).innerHTML()).includes("<code>`send`</code>"), "inline code keeps its backticks in the mono face");
    // Opens on the first edit, and the arrows step through them.
    await page.waitForFunction(() => document.querySelector("#iu-nav-label").textContent === "Edit 1 of 3");
    await page.locator("#iu-next").click();
    assertEqual(await page.locator("#iu-nav-label").innerText(), "Edit 2 of 3");
    await page.keyboard.press("Alt+ArrowUp");
    assertEqual(await page.locator("#iu-nav-label").innerText(), "Edit 1 of 3");
    assertEqual(await page.locator(".iu-map button").count(), 3, "the map has a mark for each edit");
    // The other file.
    await page.locator('.iu-tab[data-k="stu.md"]').click();
    assertEqual(await page.locator("#iu-path").innerText(), "~/myrichos/stu.md");
    assertEqual(await page.locator("#iu-nav-label").innerText(), "1 edit in stu.md");
    // The file, opened or shown, and nothing else.
    await page.locator("#iu-open").click();
    await page.locator("#iu-reveal").click();
    const opened = await page.evaluate(() => window.__invoked.filter((i) => i.name === "instructions_file_open").map((i) => i.args));
    assertEqual(JSON.stringify(opened), JSON.stringify([{ name: "stu.md", reveal: false }, { name: "stu.md", reveal: true }]));
    await page.locator("#iu-done").click();
    assertEqual(await page.locator("#instructions-update-result").isVisible(), false, "Done did not close the before and after");
    assertEqual(await page.evaluate(() => document.activeElement.id), "before", "the focus did not come back");
    assertEqual(page.__errors.length, 0, page.__errors.join("; "));
    await page.close();
    return "tabs with counts, your line tagged, edits stepped through, file opened and shown, Done closes";
  });

  await run.check("a refused update is said, with a way out", async () => {
    const page = await openPage(browser, "dark", { fail: true });
    await page.locator("#instructions-update").waitFor({ state: "visible" });
    await page.locator("#iu-ok").click();
    await page.locator("#iu-w-error").waitFor({ state: "visible" });
    assert((await page.locator("#iu-w-error").innerText()).startsWith("That could not be done"));
    await page.locator("#iu-w-close").click();
    assertEqual(await page.locator("#instructions-update").isVisible(), false);
    assertEqual(await page.locator("#instructions-update-result").count(), 0, "a failed update opened the before and after");
    await page.close();
    return "the refusal is said, Close closes";
  });

  await run.check("engine setup finishing asks again, so an upgrade's first start gets the question", async () => {
    // Second review rv-20261022T112835Z-4fb29728-8541, finding 1, with its fixture's shape: the
    // window opens with the previous engine (nothing to ask), then the app's own `runSetup`
    // (main.js, as shipped) installs the new one. Before the fix the question never came:
    // after_actual_runSetup question_shown:false.
    const fs = require("fs");
    const vm = require("vm");
    const main = fs.readFileSync(path.join(UI_DIR, "main.js"), "utf8");
    const from = main.indexOf("async function runSetup() {");
    const to = main.indexOf("/// The finished sheet's heading", from);
    assert(from > 0 && to > from, "runSetup is not where this check reads it");
    let installed = false;
    const calls = [];
    const built = [];
    const element = () => {
      const children = new Map();
      return { hidden: true, disabled: false, textContent: "", dataset: {}, style: {}, innerHTML: "",
        querySelector(s) { if (!children.has(s)) children.set(s, element()); return children.get(s); },
        querySelectorAll() { return []; }, addEventListener() {}, focus() {}, replaceChildren() {},
        getBoundingClientRect() { return { top: 0, left: 0, right: 0, bottom: 0 }; } };
    };
    const bridge = { async invoke(name) {
      calls.push(name);
      if (name === "instructions_update_question") return installed ? ASKED.slice(0, 1) : [];
      if (name === "run_setup") { installed = true; return { complete: true, status: { media_tools: { present: true } } }; }
      return null;
    } };
    const document = {
      activeElement: element(), querySelector() { return null; }, querySelectorAll() { return []; },
      createElement() { const node = element(); built.push(node); return { content: { firstChild: node } }; },
      body: { appendChild() {} }, addEventListener() {},
    };
    const context = { window: { RichBridge: bridge }, document, Bridge: bridge, setTimeout, clearTimeout, console,
      addEventListener() {}, setupDictation: () => false, refreshVoiceReadiness() {},
      invokeQuiet: (...args) => bridge.invoke(...args), paintSetupOutcome() {}, renderProviderAuth() {},
      setupRows: null, setupState: {}, setupRunning: false, videoToolsHeard: null, videoToolsLine: "",
      setupDoneWaitsForVideoTools: false };
    for (const name of ["setupGoEl", "setupLaterEl", "setupErrorEl", "setupProgressEl", "setupCloseEl",
      "setupNoteEl", "setupItemsEl", "setupAccountEl"]) context[name] = element();
    vm.createContext(context);
    vm.runInContext(fs.readFileSync(path.join(UI_DIR, "instructions-update.js"), "utf8"), context);
    for (let i = 0; i < 5; i++) await Promise.resolve();
    assertEqual(built.length, 0, "the old engine had nothing to ask, yet a sheet was built");
    vm.runInContext(main.slice(from, to), context);
    await vm.runInContext("runSetup()", context);
    for (let i = 0; i < 5; i++) await Promise.resolve();
    const asked = calls.filter((c) => c === "instructions_update_question").length;
    const shown = built.some((n) => !n.hidden);
    assertEqual(JSON.stringify({ asked, shown }), JSON.stringify({ asked: 2, shown: true }),
      "after the new engine was set up the waiting question was not shown");
    return `asked when the window opened and again after run_setup; the question is up (${asked} asks)`;
  });

  for (const theme of ["dark", "light"]) {
    await run.check(`every word of the question and the before and after clears WCAG AA in ${theme} mode`, async () => {
      const page = await openPage(browser, theme);
      await page.locator("#instructions-update").waitFor({ state: "visible" });
      const measured = [];
      const measure = async (what, selector) => {
        const nodes = await page.locator(selector).elementHandles();
        assert(nodes.length > 0, `${what}: nothing to measure`);
        for (const node of nodes) {
          const ratio = await page.evaluate((n) => window.__ratio(n), node);
          const px = await page.evaluate((n) => window.__fontPx(n), node);
          assert(ratio >= 4.5, `${theme}: ${what} is ${ratio.toFixed(2)}:1, under the 4.5:1 floor`);
          assert(px >= 16, `${theme}: ${what} is ${px}px, under the 16px readable floor`);
          measured.push(`${what} ${ratio.toFixed(2)}`);
        }
      };
      await measure("the question", "#iu-q-text");
      await measure("a file", "#iu-q-files code");
      await measure("No", "#iu-no");
      await measure("OK", "#iu-ok");
      await page.locator("#iu-ok").click();
      await measure("the update line", ".iu-q-lede");
      await page.waitForFunction(() => typeof window.__release["rich.md"] === "function");
      await page.evaluate(() => window.__release["rich.md"]());
      await page.waitForFunction(() => typeof window.__release["stu.md"] === "function");
      await page.evaluate(() => window.__release["stu.md"]());
      await page.locator("#instructions-update-result").waitFor({ state: "visible" });
      await page.evaluate(() => document.querySelectorAll(".iu-rows").forEach((r) => r.classList.remove("is-arriving")));
      for (const [what, selector] of [
        ["the title", "#iu-r-title"], ["the lede", ".iu-lede"], ["the path", "#iu-path"], ["a link", ".iu-link"],
        ["a tab", ".iu-tab code"], ["a tab count", ".iu-tab-n"], ["a column head", ".iu-cols-head small"],
        ["file text", ".iu-cell"], ["added text", ".iu-ins"], ["edited-out text", ".iu-del"], ["your line", ".iu-yours"],
        ["the nav label", "#iu-nav-label"], ["Done", "#iu-done"],
      ]) await measure(what, selector);
      await page.close();
      return measured.join("; ");
    });
  }

  await browser.close();
  process.exitCode = run.report() ? 1 : 0;
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

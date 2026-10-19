// "+ NEW CHAT OR COMPANY", AND EVERY COMPANY A SECTION HEADING — design round 32, the CEO's
// order of 2026-10-19 ("yes", to `design/mockups/rounds/round-32/new-chat-or-company.html`).
//
// His words, the acceptance criterion: "the "+ New thread" at the top must become "+ New chat or
// company" and work in a way that's intuitive and doesn't suck. Also, in front of a company name
// in the left sidebar, there needs to be a symbol of an open folder and it needs to be
// immediately visually clear in the left sidebar that a company name is the heading of a
// sub-section. Also, when hovering over the plus sign that appears when hovering over the
// company name in the left sidebar must instantly show a tooltip that says "+ New chat"."
//
// Every check below is one line of the round's README "What changed", driven through the real
// window under WebKit with the mock bridge. The pictures (`shots-32/`) are the round's seven
// states in both themes, for laying beside the mockup.
//
// TWO INSTALLS, FROM THE SAME MOCK. The mock carries six companies because other suites assert
// positions in a six-long row, so this suite narrows `navigation_tree` at the bridge to the one
// or two companies the round draws (Northwind Traders, then Lumen Labs). A company added during
// the run is never narrowed away: only the companies present at boot are filtered.
//
// Run: node new-chat-or-company.js   (or `npm test` for every suite in this directory)

"use strict";

const path = require("path");
const {
  loadPlaywright, createRun, assert, assertEqual, UI_DIR, leaveHome, openThread, shot, publishShotFile,
} = require("./lib/harness");

const APP = "file://" + path.join(UI_DIR, "index.html");
const SHOTS = path.join(__dirname, "shots-32");
const ONE = ["northwind"];
const TWO = ["northwind", "lumen"];

async function openApp(browser, opts) {
  opts = opts || {};
  const theme = opts.theme || "dark";
  const page = await browser.newPage({ viewport: opts.viewport || { width: 1440, height: 900 }, colorScheme: theme });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push("console: " + m.text());
  });
  await page.addInitScript(([t, scale]) => {
    try {
      window.localStorage.setItem("richos-theme", t);
      window.localStorage.setItem("richos-font-scale", String(scale));
      window.localStorage.setItem("richos-mock-config", JSON.stringify({ theme: t, font_scale: scale, user_name: null }));
    } catch (e) {
      /* storage unavailable: the shipped default theme */
    }
  }, [theme, opts.fontScale || 100]);
  await page.addInitScript((keep) => {
    let real = null;
    Object.defineProperty(window, "RichBridge", {
      configurable: true,
      get: () => real,
      set(v) {
        real = v;
        const invoke = v.invoke.bind(v);
        const atBoot = new Set();
        v.invoke = async (cmd, args) => {
          const out = await invoke(cmd, args);
          if (cmd === "navigation_tree" && out && out.groups) {
            if (!atBoot.size) for (const g of out.groups) atBoot.add(g.entity.id);
            const groups = out.groups.filter((g) => keep.includes(g.entity.id) || !atBoot.has(g.entity.id));
            return Object.assign({}, out, { groups });
          }
          return out;
        };
      },
    });
  }, opts.keep || TWO);
  await page.goto(APP);
  await leaveHome(page);
  await page.waitForSelector(".nav-thread", { state: "attached" });
  if (await page.evaluate(() => !document.getElementById("entity-picker").hidden)) {
    await page.keyboard.press("Escape");
    await page.waitForSelector("#entity-picker", { state: "hidden" });
  }
  await openThread(page, "acme");
  page.__errors = errors;
  return page;
}

const menuState = (page) =>
  page.evaluate(() => {
    const m = document.getElementById("new-chat-menu");
    const btn = document.getElementById("rail-new-thread");
    if (!m) return { exists: false };
    const r = m.getBoundingClientRect();
    const b = btn.getBoundingClientRect();
    return {
      exists: true,
      open: !m.hidden && getComputedStyle(m).display !== "none",
      form: m.classList.contains("is-form"),
      label: (m.querySelector(".m-label") || {}).textContent || "",
      items: [...m.querySelectorAll(".m-item")].map((i) => ({
        name: (i.querySelector(".m-name") || i).textContent.trim(),
        kbd: (i.querySelector("kbd") || {}).textContent || "",
        company: i.dataset.company || null,
        folder: !!i.querySelector("svg"),
      })),
      focused: document.activeElement && document.activeElement.closest("#new-chat-menu")
        ? ((document.activeElement.querySelector(".m-name") || document.activeElement).textContent || "").trim()
        : null,
      gapBelowButton: Math.round(r.top - b.bottom),
      leftAligned: Math.round(r.left - b.left),
      expanded: btn.getAttribute("aria-expanded"),
    };
  });

const newChatScreen = (page) =>
  page.evaluate(() => {
    const view = document.getElementById("entity-view");
    const scope = document.getElementById("composer-scope");
    const draft = document.querySelector(".nav-thread-row.is-draft");
    const section = draft && draft.closest(".nav-group--entity");
    return {
      onScreen: !view.hidden,
      name: document.getElementById("entity-view-name").textContent.trim(),
      scope: scope && !scope.hidden ? scope.textContent.trim() : "",
      crumb: document.getElementById("scope-thread").textContent.trim(),
      draft: draft ? draft.textContent.trim() : null,
      draftDot: !!(draft && draft.querySelector(".draft-dot")),
      draftActive: !!(draft && draft.classList.contains("is-active")),
      draftFirst: !!(section && section.querySelector(".nav-thread-row") === draft),
      draftCompany: section ? section.querySelector(".co-name").textContent.trim() : null,
      otherActive: [...document.querySelectorAll(".nav-thread-row.is-active:not(.is-draft)")].length,
      canType: !document.getElementById("input").disabled,
    };
  });

async function main() {
  const run = createRun('"+ New chat or company", and companies as section headings (round 32, CEO 2026-10-19)');
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();

  await run.check('the button reads "+ New chat or company", with its ⌘N key beside it like Search\'s ⌘K', async () => {
    const page = await openApp(browser);
    const b = await page.evaluate(() => {
      const btn = document.getElementById("rail-new-thread");
      const kbd = btn.querySelector("kbd.rail-kbd");
      return {
        label: btn.querySelector(".rail-action-label").textContent.trim(),
        kbd: kbd ? kbd.textContent.trim() : null,
        popup: btn.getAttribute("aria-haspopup"),
        keys: btn.getAttribute("aria-keyshortcuts"),
      };
    });
    assertEqual(b.label, "+ New chat or company", "the button's words");
    assertEqual(b.kbd, "⌘N", "the key shown beside it");
    assertEqual(b.popup, "menu", "it says it opens a menu");
    assert(/Meta\+N/.test(b.keys || ""), "its shortcut is not declared: " + b.keys);
    assertEqual(page.__errors, [], "page errors");
    await page.close();
    return JSON.stringify(b);
  });

  await run.check('a click opens a small menu right under it: "New chat in", each company, then "New company"', async () => {
    const page = await openApp(browser);
    await page.click("#rail-new-thread");
    const m = await menuState(page);
    assert(m.exists && m.open, "no menu opened under the button");
    assertEqual(m.label.trim(), "New chat in", "the menu's heading");
    assertEqual(m.items.map((i) => i.name), ["Northwind Traders", "Lumen Labs", "New company"], "the menu's rows");
    assert(m.items.every((i) => i.folder), "a row has no folder symbol");
    assert(m.items.every((i) => !i.kbd), "with two companies no row carries ⌘N");
    assert(m.gapBelowButton >= 0 && m.gapBelowButton <= 10, "the menu is not right under the button: gap " + m.gapBelowButton);
    assert(Math.abs(m.leftAligned) <= 2, "the menu is not aligned with the button: " + m.leftAligned);
    assertEqual(m.expanded, "true", "aria-expanded on the button");
    await page.click("#rail-new-thread");
    assertEqual((await menuState(page)).open, false, "a second click closes it");
    await page.close();
    return "rows " + m.items.map((i) => i.name).join(", ") + "; " + m.gapBelowButton + "px under the button";
  });

  await run.check('one click on a company starts a new chat there: a "New chat" row with a gold dot tops its section', async () => {
    const page = await openApp(browser);
    await page.click("#rail-new-thread");
    await page.click('#new-chat-menu .m-item[data-company="lumen"]');
    const s = await newChatScreen(page);
    assertEqual((await menuState(page)).open, false, "the menu stays open after the pick");
    assert(s.onScreen, "the company's new-chat screen is not on screen");
    assertEqual(s.name, "Lumen Labs", "the screen's company");
    assertEqual(s.scope, "New chat in Lumen Labs", "the line above the composer");
    assertEqual(s.crumb, "New chat", "the breadcrumb");
    assertEqual(s.draft, "New chat", "the row at the top of the section");
    assert(s.draftDot && s.draftActive && s.draftFirst, "the New chat row: " + JSON.stringify(s));
    assertEqual(s.draftCompany, "Lumen Labs", "the section the row is in");
    assertEqual(s.otherActive, 0, "another chat is still marked as the one on screen");
    assert(s.canType, "the composer is off");
    const ink = await page.evaluate(() => {
      const dot = document.querySelector(".nav-thread-row.is-draft .draft-dot");
      const probe = document.createElement("span");
      probe.style.color = "var(--gold-text)";
      document.body.appendChild(probe);
      const gold = getComputedStyle(probe).color;
      probe.remove();
      return { dot: getComputedStyle(dot).backgroundColor, gold };
    });
    assertEqual(ink.dot, ink.gold, "the dot is not the app's gold");
    assertEqual(page.__errors, [], "page errors");
    await page.close();
    return JSON.stringify(s);
  });

  await run.check("with one company, its row carries ⌘N, and ⌘N starts a new chat there straight away", async () => {
    const page = await openApp(browser, { keep: ONE });
    await page.click("#rail-new-thread");
    const m = await menuState(page);
    assertEqual(m.items.map((i) => [i.name, i.kbd]), [["Northwind Traders", "⌘N"], ["New company", ""]], "the menu with one company");
    await page.keyboard.press("Escape");
    assertEqual((await menuState(page)).open, false, "Escape did not close the menu");
    await page.keyboard.press("Meta+n");
    const s = await newChatScreen(page);
    assertEqual((await menuState(page)).open, false, "⌘N opened the menu with one company");
    assert(s.onScreen && s.name === "Northwind Traders" && s.draft === "New chat", "⌘N did not start a chat: " + JSON.stringify(s));
    await page.close();
    return "⌘N -> " + s.scope;
  });

  await run.check("⌘N with two companies opens the menu on the first; arrows move, Enter picks, Escape closes", async () => {
    const page = await openApp(browser);
    await page.keyboard.press("Meta+n");
    let m = await menuState(page);
    assert(m.open, "⌘N did not open the menu with two companies");
    assertEqual(m.focused, "Northwind Traders", "focus on open");
    const walk = [];
    for (const key of ["ArrowDown", "ArrowDown", "ArrowDown", "ArrowUp", "End", "Home"]) {
      await page.keyboard.press(key);
      walk.push(key + "->" + (await menuState(page)).focused);
    }
    assertEqual(
      walk,
      ["ArrowDown->Lumen Labs", "ArrowDown->New company", "ArrowDown->Northwind Traders", "ArrowUp->New company",
        "End->New company", "Home->Northwind Traders"],
      "arrow keys"
    );
    await page.keyboard.press("Escape");
    m = await menuState(page);
    assertEqual(m.open, false, "Escape did not close the menu");
    const back = await page.evaluate(() => document.activeElement && document.activeElement.id);
    assertEqual(back, "rail-new-thread", "focus after Escape");
    await page.keyboard.press("Meta+n");
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("Enter");
    const s = await newChatScreen(page);
    assert(s.onScreen && s.name === "Lumen Labs", "Enter did not pick Lumen Labs: " + JSON.stringify(s));
    await page.close();
    return walk.join(", ") + "; Enter -> " + s.scope;
  });

  // Mid-job review rv-20261019T132219Z-c8964a8e-f9d9: with the sidebar hidden the menu hung
  // from the hidden button and opened off the left edge (menu_left=-284, 20px of it visible).
  await run.check("⌘N with the sidebar hidden opens the whole menu on screen, under the sidebar toggle", async () => {
    const page = await openApp(browser);
    await page.keyboard.press("Meta+Shift+s");
    await page.waitForFunction(() => document.body.classList.contains("rail-closed"));
    await page.keyboard.press("Meta+n");
    const m = await page.evaluate(() => {
      const menu = document.getElementById("new-chat-menu");
      const r = menu.getBoundingClientRect();
      const t = document.getElementById("rail-toggle").getBoundingClientRect();
      const a = document.activeElement;
      return {
        open: !menu.hidden,
        left: Math.round(r.left), right: Math.round(r.right), top: Math.round(r.top), bottom: Math.round(r.bottom),
        vw: window.innerWidth, vh: window.innerHeight,
        underToggle: Math.round(r.top - t.bottom),
        focused: a && a.closest("#new-chat-menu") ? a.textContent.trim() : null,
      };
    });
    assert(m.open, "⌘N did not open the menu with the sidebar hidden");
    assert(m.left >= 0 && m.right <= m.vw && m.top >= 0 && m.bottom <= m.vh, "the menu is not wholly on screen: " + JSON.stringify(m));
    assert(m.underToggle >= 0 && m.underToggle <= 10, "the menu is not under the sidebar toggle: " + JSON.stringify(m));
    assertEqual(m.focused, "Northwind Traders", "focus on open");
    await page.keyboard.press("Escape");
    assertEqual(await page.evaluate(() => document.activeElement && document.activeElement.id), "rail-toggle", "focus after Escape");
    await page.keyboard.press("Meta+n");
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("Enter");
    const s = await newChatScreen(page);
    assert(s.onScreen && s.name === "Lumen Labs", "Enter did not start a chat in Lumen Labs: " + JSON.stringify(s));
    assertEqual(page.__errors, [], "page errors");
    await page.close();
    return "menu at x " + m.left + "-" + m.right + " of " + m.vw + ", " + m.underToggle + "px under the toggle";
  });

  // Mid-job review rv-20261019T142223Z-c855f9a6-d96d, finding 1: opened with the pointer, the
  // arrow keys did nothing, because focus stayed on the button and the arrows were the menu's.
  await run.check("opened with the pointer, the first arrow key moves into the menu and Enter picks", async () => {
    const page = await openApp(browser);
    await page.click("#rail-new-thread");
    await page.keyboard.press("ArrowDown");
    const first = await menuState(page);
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("Enter");
    const s = await newChatScreen(page);
    assertEqual(first.focused, "Northwind Traders", "the first arrow after a click");
    assert(s.onScreen && s.name === "Lumen Labs", "ArrowDown, ArrowDown, Enter did not start a chat in Lumen Labs: " + JSON.stringify(s));
    await page.close();
    return "click, ArrowDown -> " + first.focused + ", ArrowDown, Enter -> " + s.scope;
  });

  // The same review, finding 2: at 150% text in the app's smallest window (1024 x 700) the
  // company form with its refusal ran below the foot of the window, Add and Cancel with it.
  await run.check("at 150% text in a 1024 x 700 window the company form with its refusal stays in the window, Add and Cancel reachable", async () => {
    const page = await openApp(browser, { keep: ONE, fontScale: 150, viewport: { width: 1024, height: 700 } });
    await page.click("#rail-new-thread");
    await page.click("#new-chat-menu .m-item[data-new-company]");
    await page.click("#entity-add-go");
    await page.waitForSelector("#new-chat-menu #entity-add-error:not([hidden])");
    const box = await page.evaluate(() => {
      const r = document.getElementById("new-chat-menu").getBoundingClientRect();
      return { bottom: Math.round(r.bottom), vh: window.innerHeight, scale: getComputedStyle(document.documentElement).fontSize };
    });
    assert(box.bottom <= box.vh, "the menu runs below the window: " + JSON.stringify(box));
    // Reachable the way a hand reaches them: Playwright scrolls the menu to the control, and
    // the click has to land on it and do its job.
    await page.click("#entity-add-cancel");
    assertEqual((await menuState(page)).open, false, "Cancel, scrolled to, did not close the menu");
    await page.close();
    return "menu bottom " + box.bottom + " of " + box.vh + " at root font " + box.scale;
  });

  await run.check('"New company" turns the same menu into the app\'s company form, with a back arrow to the menu', async () => {
    const page = await openApp(browser);
    await page.click("#rail-new-thread");
    await page.click("#new-chat-menu .m-item[data-new-company]");
    const f = await page.evaluate(() => {
      const m = document.getElementById("new-chat-menu");
      const form = document.getElementById("entity-add");
      const vis = (n) => !!n && !n.hidden && n.getClientRects().length > 0;
      return {
        open: !m.hidden,
        isForm: m.classList.contains("is-form"),
        title: (m.querySelector(".m-title") || {}).textContent || "",
        inMenu: m.contains(form) && vis(form),
        labels: [...form.querySelectorAll(".entity-add-label")].map((l) => l.textContent.replace(/\s+/g, " ").trim()),
        hint: form.querySelector(".entity-add-hint").textContent.trim(),
        go: vis(document.getElementById("entity-add-go")),
        cancel: vis(document.getElementById("entity-add-cancel")),
        back: vis(m.querySelector(".m-back")),
        focus: document.activeElement && document.activeElement.id,
        pickerOpen: !document.getElementById("entity-picker").hidden,
      };
    });
    assert(f.open && f.isForm && f.inMenu, "the form is not in the menu: " + JSON.stringify(f));
    assertEqual(f.title.trim(), "New company", "the form's title");
    assertEqual(f.labels, ["What's the company called?", "Its folder on this Mac (optional)"], "the form's labels");
    assertEqual(f.hint, "You can leave this blank.", "the folder hint");
    assert(f.go && f.cancel && f.back, "Add, Cancel or the back arrow is missing: " + JSON.stringify(f));
    assertEqual(f.focus, "entity-add-name", "focus in the form");
    assertEqual(f.pickerOpen, false, "the old company dialog opened");
    await page.click("#new-chat-menu .m-back");
    const m = await menuState(page);
    assert(m.open && !m.form, "the back arrow did not return to the menu");
    assertEqual(m.focused, "Northwind Traders", "focus after going back");
    await page.click("#new-chat-menu .m-item[data-new-company]");
    await page.click("#entity-add-cancel");
    assertEqual((await menuState(page)).open, false, "Cancel did not close it");
    await page.close();
    return JSON.stringify(f);
  });

  await run.check("Add with no name refuses in the app's words, typing clears it; Enter adds; the company arrives with a new chat open", async () => {
    const page = await openApp(browser, { keep: ONE });
    await page.click("#rail-new-thread");
    await page.click("#new-chat-menu .m-item[data-new-company]");
    await page.click("#entity-add-go");
    await page.waitForSelector("#new-chat-menu #entity-add-error:not([hidden])");
    const refusal = (await page.textContent("#entity-add-error")).trim();
    assert(/^I need a name for the company before I can file anything under it\./.test(refusal), "the refusal: " + refusal);
    await page.keyboard.type("Orchard Bay");
    assertEqual(await page.isHidden("#entity-add-error"), true, "typing did not clear the refusal");
    await page.keyboard.press("Enter");
    await page.waitForSelector('.nav-group--entity[data-entity="orchard-bay"]');
    const s = await newChatScreen(page);
    const arrived = await page.evaluate(() => {
      const g = document.querySelector('.nav-group--entity[data-entity="orchard-bay"]');
      return { isNew: g.classList.contains("is-new"), name: g.querySelector(".co-name").textContent.trim() };
    });
    assertEqual((await menuState(page)).open, false, "the menu stayed open after adding");
    assertEqual(arrived.name, "Orchard Bay", "the new section's heading");
    assert(arrived.isNew, "the new company did not arrive with its light");
    assert(s.onScreen && s.name === "Orchard Bay" && s.draftCompany === "Orchard Bay", "no new chat open in it: " + JSON.stringify(s));
    assertEqual(s.scope, "New chat in Orchard Bay", "the line above the composer");
    assert(s.canType, "the composer is off");
    assertEqual(page.__errors, [], "page errors");
    await page.close();
    return "refused with \"" + refusal.slice(0, 60) + "…\"; then " + s.scope;
  });

  await run.check("a company is a section heading: an open folder before its name, full ink at 16px, its chats hanging underneath", async () => {
    const page = await openApp(browser);
    const h = await page.evaluate(() => {
      const probe = document.createElement("span");
      probe.style.color = "var(--ink)";
      document.body.appendChild(probe);
      const ink = getComputedStyle(probe).color;
      probe.style.color = "var(--gold-text)";
      const gold = getComputedStyle(probe).color;
      probe.remove();
      return [...document.querySelectorAll("#rail-nav .nav-group--entity")].map((g, i) => {
        const label = g.querySelector(".nav-group-label");
        const icon = label.querySelector("svg.co-folder");
        const name = label.querySelector(".co-name");
        const list = g.querySelector(".nav-threads");
        const ls = getComputedStyle(list);
        const ns = getComputedStyle(name);
        return {
          name: name.textContent.trim(),
          folder: icon ? icon.dataset.folder : null,
          folderFirst: !!icon && label.firstElementChild === icon,
          folderGold: icon ? getComputedStyle(icon).color === gold : false,
          size: ns.fontSize,
          ink: ns.color === ink,
          weight: Number(ns.fontWeight),
          caps: ns.textTransform,
          indent: Math.round(list.getBoundingClientRect().left - label.getBoundingClientRect().left),
          guide: ls.borderLeftStyle + " " + ls.borderLeftWidth,
          rule: i > 0 ? getComputedStyle(g).borderTopStyle : null,
        };
      });
    });
    assertEqual(h.map((x) => x.name), ["Northwind Traders", "Lumen Labs"], "the headings");
    for (const x of h) {
      assertEqual(x.folder, "open", x.name + ": the folder");
      assert(x.folderFirst, x.name + ": the folder is not in front of the name");
      assert(x.folderGold, x.name + ": the folder is not the app's gold");
      assertEqual(x.size, "16px", x.name + ": size");
      assert(x.ink, x.name + ": not in full ink");
      assert(x.weight >= 600, x.name + ": weight " + x.weight);
      assertEqual(x.caps, "none", x.name + ": capitals");
      assert(x.indent >= 12, x.name + ": its chats are not indented under it (" + x.indent + "px)");
      assertEqual(x.guide, "solid 1px", x.name + ": the guide line");
    }
    assertEqual(h[1].rule, "solid", "no rule between one company and the next");
    // Collapsed: a closed folder, and the chats gone.
    await page.hover('.nav-group--entity[data-entity="lumen"] .nav-group-head');
    await page.click('.nav-group--entity[data-entity="lumen"] .nav-collapse');
    const c = await page.evaluate(() => {
      const g = document.querySelector('.nav-group--entity[data-entity="lumen"]');
      const list = g.querySelector(".nav-threads");
      return { folder: g.querySelector("svg.co-folder").dataset.folder, listShown: !list.hidden && list.getClientRects().length > 0 };
    });
    assertEqual(c, { folder: "closed", listShown: false }, "a collapsed company");
    await page.click('.nav-group--entity[data-entity="lumen"] .nav-collapse');
    await page.close();
    return JSON.stringify(h[0]);
  });

  await run.check('hovering a company shows collapse and the plus; the plus shows "+ New chat" in the same frame, no delay, no fade', async () => {
    const page = await openApp(browser);
    const head = '.nav-group--entity[data-entity="northwind"] .nav-group-head';
    const tools = () =>
      page.evaluate((sel) => {
        const h = document.querySelector(sel);
        // Painted at all: laid out, not hidden, and no transparent ancestor up to the heading
        // (opacity is not inherited, so the control's own value says nothing about its row's).
        const vis = (n) => {
          if (!n || !n.getClientRects().length || getComputedStyle(n).visibility === "hidden") return false;
          for (let a = n; a && a !== h.parentElement; a = a.parentElement) if (Number(getComputedStyle(a).opacity) === 0) return false;
          return true;
        };
        return { collapse: vis(h.querySelector(".nav-collapse")), plus: vis(h.querySelector(".nav-group-add")) };
      }, head);
    await page.mouse.move(700, 450);
    const before = await tools();
    await page.hover(head);
    const during = await tools();
    assertEqual(before, { collapse: false, plus: false }, "the controls show without a hover");
    assertEqual(during, { collapse: true, plus: true }, "the controls on hover");
    // THE SAME FRAME: the pointer's own event is dispatched and the tooltip read in the same
    // task, before the browser could paint even once. Visible there means visible in the frame
    // the pointer arrives in.
    const sync = await page.evaluate((sel) => {
      const plus = document.querySelector(sel + " .nav-group-add");
      const r = plus.getBoundingClientRect();
      plus.dispatchEvent(new PointerEvent("pointerover", { bubbles: true, clientX: r.left + 4, clientY: r.top + 4 }));
      const tip = document.getElementById("rail-tip");
      const cs = tip ? getComputedStyle(tip) : null;
      return tip && {
        shown: !tip.hidden && cs.display !== "none" && cs.visibility !== "hidden",
        text: tip.textContent.trim(),
        opacity: cs.opacity,
        transition: cs.transitionDuration,
        delay: cs.transitionDelay,
        animation: cs.animationName,
        describes: plus.getAttribute("aria-describedby") === "rail-tip",
      };
    }, head);
    assert(sync, "there is no tooltip element at all");
    assert(sync.shown, "the tooltip is not on screen in the frame the pointer arrives in");
    assertEqual(sync.text, "+ New chat", "the tooltip's words");
    assertEqual(sync.opacity, "1", "the tooltip fades in");
    assert(/^0s(, 0s)*$/.test(sync.transition) && /^0s(, 0s)*$/.test(sync.delay), "the tooltip has a transition: " + sync.transition + " / " + sync.delay);
    assertEqual(sync.animation, "none", "the tooltip animates in");
    assert(sync.describes, "the plus is not described by the tooltip");
    // The real pointer, too, and the tooltip sits beside the plus.
    await page.mouse.move(700, 450);
    assertEqual(await page.isHidden("#rail-tip"), true, "the tooltip stays after the pointer leaves");
    // The way a hand gets there: onto the company, then onto its plus.
    await page.hover(head);
    await page.hover(head + " .nav-group-add");
    const real = await page.evaluate((sel) => {
      const tip = document.getElementById("rail-tip");
      const p = document.querySelector(sel + " .nav-group-add").getBoundingClientRect();
      const t = tip.getBoundingClientRect();
      return { shown: !tip.hidden, text: tip.textContent.trim(), right: Math.round(t.left - p.right), midDelta: Math.round((t.top + t.height / 2) - (p.top + p.height / 2)) };
    }, head);
    assert(real.shown && real.text === "+ New chat", "the real hover: " + JSON.stringify(real));
    assert(real.right > 0 && real.right <= 16 && Math.abs(real.midDelta) <= 2, "the tooltip is not beside the plus: " + JSON.stringify(real));
    // Keyboard focus shows it too. The plus has to be focusable while it is not shown (a
    // `display: none` control is out of the focus order, so focus could never land on it to
    // show it); WebKit's own Tab order skips buttons unless Full Keyboard Access is on, so the
    // focus is placed directly, which is the event either path delivers.
    await page.mouse.move(700, 450);
    await page.focus(head + " .nav-group-add");
    const kb = await page.evaluate(() => {
      const a = document.activeElement;
      const tip = document.getElementById("rail-tip");
      return { plus: !!a && a.classList.contains("nav-group-add"), tip: !tip.hidden, text: tip.textContent.trim() };
    });
    assertEqual(kb, { plus: true, tip: true, text: "+ New chat" }, "the plus by keyboard");
    await page.close();
    return "shown in the same task as the pointer event; transition " + sync.transition + "; beside the plus at +" + real.right + "px";
  });

  await run.check("the plus starts a new chat in that company; the name opens the company's overview, its chats called Chats", async () => {
    const page = await openApp(browser);
    await page.hover('.nav-group--entity[data-entity="lumen"] .nav-group-head');
    await page.click('.nav-group--entity[data-entity="lumen"] .nav-group-add');
    const s = await newChatScreen(page);
    assert(s.onScreen && s.name === "Lumen Labs" && s.draftCompany === "Lumen Labs", "the plus: " + JSON.stringify(s));
    await page.click('.nav-group--entity[data-entity="northwind"] .nav-group-label');
    const o = await page.evaluate(() => ({
      name: document.getElementById("entity-view-name").textContent.trim(),
      scope: document.getElementById("composer-scope").textContent.trim(),
      draft: !!document.querySelector(".nav-thread-row.is-draft"),
      facts: [...document.querySelectorAll("#entity-view-facts dt")].map((d) => d.textContent.trim()),
      block: document.querySelector("#entity-view-threads .entity-block-title").textContent.trim(),
    }));
    assertEqual(o.name, "Northwind Traders", "the overview's company");
    assertEqual(o.scope, "Talk to Rich about Northwind Traders", "the overview's composer line");
    assertEqual(o.draft, false, "the New chat row stays after leaving it");
    assertEqual(o.facts[0], "Chats", "the overview's count");
    assertEqual(o.block, "Chats", "the overview's list title");
    await page.close();
    return JSON.stringify(o);
  });

  // ---- the seven states of the round, both themes, beside the mockup ---------------------
  const STATES = {
    one: { keep: ONE, drive: async () => {} },
    two: { keep: TWO, drive: async () => {} },
    menu: { keep: TWO, drive: async (p) => { await p.click("#rail-new-thread"); } },
    newchat: { keep: TWO, drive: async (p) => { await p.click("#rail-new-thread"); await p.click('#new-chat-menu .m-item[data-company="lumen"]'); } },
    form: {
      keep: ONE,
      drive: async (p) => {
        await p.click("#rail-new-thread");
        await p.click("#new-chat-menu .m-item[data-new-company]");
        await p.click("#entity-add-go");
        await p.waitForSelector("#entity-add-error:not([hidden])");
      },
    },
    added: {
      keep: ONE,
      drive: async (p) => {
        await p.click("#rail-new-thread");
        await p.click("#new-chat-menu .m-item[data-new-company]");
        await p.fill("#entity-add-name", "Lumen Labs");
        await p.press("#entity-add-name", "Enter");
        await p.waitForSelector('.nav-group--entity[data-entity="lumen-labs"]');
      },
    },
    tooltip: {
      keep: TWO,
      drive: async (p) => {
        await p.hover('.nav-group--entity[data-entity="northwind"] .nav-group-head');
        await p.hover('.nav-group--entity[data-entity="northwind"] .nav-group-add');
        await p.waitForSelector("#rail-tip:not([hidden])");
      },
    },
  };
  for (const [state, spec] of Object.entries(STATES)) {
    await run.check("picture: " + state + ", dark and light (shots-32/)", async () => {
      const out = [];
      for (const theme of ["dark", "light"]) {
        const page = await openApp(browser, { keep: spec.keep, theme });
        if (state !== "tooltip") await page.mouse.move(720, 600);
        await spec.drive(page);
        // The pointer leaves whatever it last pressed, so no picture catches a hover by chance
        // (form-light differed in the hovered Add button, x 33-193 y 480-513, on the 2nd run).
        if (state !== "tooltip") await page.mouse.move(720, 600);
        const s = await shot(page, "r32-" + state + "-" + theme, { fullPage: false });
        publishShotFile(s.file, path.join(SHOTS, state + "-" + theme + ".png"));
        assertEqual(page.__errors, [], state + "/" + theme + ": page errors");
        out.push(theme + " " + s.distinct + " colors");
        await page.close();
      }
      return out.join(", ");
    });
  }

  await browser.close();
  process.exit(run.report() > 0 ? 1 : 0);
}

main().catch((e) => {
  console.error(e && e.stack ? e.stack : e);
  process.exit(1);
});

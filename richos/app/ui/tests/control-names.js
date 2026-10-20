// **EVERY CONTROL ON THE COMPOSER HAS A NAME, AND THE NAME IS NOT A SHAPE** — candidate-.2
// defect #7.
//
// `docs/verification/2026-09-17-nightly-1.2.0-20260917.2-onscreen-audit-2.md` §4:
//
//   *"#7 — the send control has no accessible name. Read live from the accessibility tree.
//    `AXButton` with `AXTitle = "▷"` and an empty `AXDescription`; the string "Send" exists
//    only as `AXHelp`, a tooltip. A screen-reader user hears a geometric shape. The talk
//    control right beside it is named correctly, which is the control. The composer itself is
//    unnamed but does expose `AXPlaceholderValue = "Talk to Rich…"`."*
//
// ## HOW THE NAME IS COMPUTED HERE, AND WHAT THAT CANNOT SEE
//
// Playwright 1.61 ships no accessibility snapshot API, so this suite computes the name from
// the DOM by the accname path that applies to these elements: `aria-labelledby`, then
// `aria-label`, then the element's own contents with `aria-hidden` subtrees removed (for a
// button) or its `<label>` (for a field). **`title` is computed SEPARATELY and never counted**,
// which is the whole point of this defect: "Send" in a tooltip is what the walk found, and a
// check that folded `title` into the name would have called the broken state fine.
//
// This is an approximation of one platform's tree and it is declared as one. What it cannot
// tell you is how VoiceOver renders the result; what it CAN tell you, which is the thing that
// was wrong, is whether a name exists at all without reaching for the tooltip.
//
// ## THE NAME MUST ALSO HOLD STILL
//
// The composer's name used to come from its placeholder, and the placeholder is STATE — it
// reads "Send is off until I know the company" while a company block is up. So the control
// was being renamed underneath a screen-reader user as the app changed condition. This suite
// reads the name in both states and asserts they are the same string.
//
// Run: node control-names.js   (or `npm test` for every suite in this directory)

"use strict";

const path = require("path");
const { loadPlaywright, leaveHome, createRun, assert, assertEqual, UI_DIR } = require("./lib/harness");

const APP = "file://" + path.join(UI_DIR, "index.html");

/// At least one letter. `▷`, `↓`, `·` and `…` are not names; "Send" is.
const HAS_A_WORD = /\p{L}/u;

/// The accname walk, in the page. Deliberately small and deliberately missing `title`.
const NAME_WALK = `(() => {
  function visibleText(el) {
    let out = "";
    for (const n of el.childNodes) {
      if (n.nodeType === 3) { out += n.nodeValue; continue; }
      if (n.nodeType !== 1) continue;
      if (n.getAttribute("aria-hidden") === "true") continue;
      if (n.hidden) continue;
      out += visibleText(n);
    }
    return out.replace(/\\s+/g, " ").trim();
  }
  function accName(el) {
    const by = el.getAttribute("aria-labelledby");
    if (by) {
      const parts = by.split(/\\s+/).map((id) => document.getElementById(id)).filter(Boolean);
      if (parts.length) return { name: parts.map(visibleText).join(" ").trim(), from: "aria-labelledby" };
    }
    const label = el.getAttribute("aria-label");
    if (label && label.trim()) return { name: label.trim(), from: "aria-label" };
    if (el.id) {
      const lab = document.querySelector('label[for="' + el.id + '"]');
      if (lab) return { name: visibleText(lab), from: "label" };
    }
    const tag = el.tagName.toLowerCase();
    if (tag === "button" || el.getAttribute("role") === "button") {
      const t = visibleText(el);
      if (t) return { name: t, from: "contents" };
    }
    if (tag === "textarea" || tag === "input") {
      const ph = el.getAttribute("placeholder");
      if (ph && ph.trim()) return { name: ph.trim(), from: "placeholder" };
    }
    return { name: "", from: "none" };
  }
  const scope = document.getElementById("composer-row");
  const controls = Array.from(scope.querySelectorAll("button, textarea, input, select, [role=button]"));
  return controls
    .filter((el) => !el.closest("[hidden]"))
    .map((el) => {
      const n = accName(el);
      return {
        id: el.id || el.className || el.tagName.toLowerCase(),
        tag: el.tagName.toLowerCase(),
        name: n.name,
        from: n.from,
        title: el.getAttribute("title") || "",
        hidden: el.hidden,
      };
    });
})()`;

async function openApp(browser, preset) {
  const page = await browser.newPage({ viewport: { width: 1400, height: 900 } });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push("console: " + m.text());
  });
  page.__errors = errors;
  if (preset) {
    await page.addInitScript((v) => {
      window.__RICHOS_MOCK_PRESET__ = v;
    }, preset);
  }
  await page.goto(APP);
  await leaveHome(page);
  await page.waitForFunction("typeof window.RichTimeline === 'object'");
  return page;
}

async function main() {
  const run = createRun("the composer's controls: named, and not by their tooltips");
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();
  const page = await openApp(browser, null);

  await run.check("NEGATIVE CONTROL: the walk found the controls it is supposed to name", async () => {
    const got = await page.evaluate(NAME_WALK);
    const ids = got.map((c) => c.id);
    for (const want of ["talk-toggle", "input", "send"]) {
      assert(ids.indexOf(want) >= 0, `the walk never saw #${want}; it saw ${JSON.stringify(ids)}`);
    }
    assert(got.length >= 3, `only ${got.length} control(s) in #composer-row`);
    return `${got.length} control(s): ${ids.join(", ")}`;
  });

  await run.check("every visible control in the composer has a name with a word in it", async () => {
    const got = await page.evaluate(NAME_WALK);
    const nameless = got.filter((c) => !c.name);
    const shapes = got.filter((c) => c.name && !HAS_A_WORD.test(c.name));
    assertEqual(nameless.map((c) => c.id), [], "controls with no accessible name at all");
    assertEqual(
      shapes.map((c) => `${c.id} = ${JSON.stringify(c.name)}`),
      [],
      "a name made only of shapes is what a screen reader reads out loud"
    );
    return got.map((c) => `${c.id} = ${JSON.stringify(c.name)} (${c.from})`).join(" · ");
  });

  await run.check("no control leaves its real name in the TOOLTIP", async () => {
    // The defect, exactly: `title="Send"` on a button whose computed name was "▷". The first
    // version of this check asked only whether the name was MISSING while a tooltip existed,
    // and #send passed it — it had a name, the name was a shape. So the test is the one that
    // catches the shape: a control whose name carries no word while its tooltip carries one
    // has its real name in `AXHelp`, which is where the walk found "Send".
    const got = await page.evaluate(NAME_WALK);
    const stranded = got.filter((c) => c.title && HAS_A_WORD.test(c.title) && !HAS_A_WORD.test(c.name || ""));
    assertEqual(
      stranded.map((c) => `${c.id}: name ${JSON.stringify(c.name)}, tooltip ${JSON.stringify(c.title)}`),
      [],
      "a tooltip is not an accessible name — it is `AXHelp`, and this is defect #7 itself"
    );
    const withTooltip = got.filter((c) => c.title);
    return `${withTooltip.length} control(s) carry a tooltip (${withTooltip.map((c) => c.id).join(", ") || "none"}); none of them is where the name lives`;
  });

  await run.check("the send control's name is a word, and the glyph is out of the computation", async () => {
    const r = await page.evaluate(() => {
      const b = document.getElementById("send");
      const g = b.querySelector(".send-glyph");
      return {
        label: b.getAttribute("aria-label"),
        glyph: g ? g.textContent.trim() : null,
        glyphHidden: g ? g.getAttribute("aria-hidden") : null,
        title: b.getAttribute("title"),
      };
    });
    assertEqual(r.label, "Send", "the send control's name");
    assertEqual(r.glyphHidden, "true", `the glyph ${JSON.stringify(r.glyph)} is still in the name computation`);
    assertEqual(r.title, "Send", "the pointer tooltip is unchanged");
    return `aria-label "Send", glyph ${JSON.stringify(r.glyph)} aria-hidden, tooltip still "Send"`;
  });

  await run.check("CONTROL: the talk control beside it was already right, and still is", async () => {
    // The audit's own control. If this one ever goes the way #send did, the fix above was a
    // patch on one button rather than a rule.
    const r = await page.evaluate(() => {
      const b = document.getElementById("talk-toggle");
      return { label: b.getAttribute("aria-label"), pressed: b.getAttribute("aria-pressed") };
    });
    assert(r.label && HAS_A_WORD.test(r.label), `the talk control's name is ${JSON.stringify(r.label)}`);
    assertEqual(r.pressed, "false", "the talk control's state is still exposed beside its name");
    return `aria-label ${JSON.stringify(r.label)}, aria-pressed ${r.pressed}`;
  });

  await run.check("the composer's name holds still while its state changes", async () => {
    const live = await page.evaluate(() => {
      const i = document.getElementById("input");
      return { name: i.getAttribute("aria-label"), placeholder: i.placeholder };
    });
    assert(live.name && HAS_A_WORD.test(live.name), `the composer's name is ${JSON.stringify(live.name)}`);
    assert(
      live.name !== live.placeholder,
      "the name and the placeholder are the same string, so the name is state and will move with it"
    );

    // The same box, in the state that used to rename it.
    const blocked = await openApp(browser, { chosenEntity: null });
    await blocked.waitForSelector("#entity-picker:not([hidden])");
    await blocked.click("#entity-picker-later");
    await blocked.waitForSelector("#entity-picker", { state: "hidden" });
    const off = await blocked.evaluate(() => {
      const i = document.getElementById("input");
      return { name: i.getAttribute("aria-label"), placeholder: i.placeholder, disabled: i.disabled };
    });
    assert(off.disabled, "this check needs the blocked state and did not reach it");
    assertEqual(off.name, live.name, "the composer was renamed when send was switched off");
    assert(off.placeholder !== live.placeholder, "the placeholder did not change, so this proves nothing");
    assertEqual(blocked.__errors, [], "the page reported errors");
    await blocked.close();
    return `name ${JSON.stringify(live.name)} in both states; placeholder moved ${JSON.stringify(live.placeholder)} -> ${JSON.stringify(off.placeholder)}`;
  });

  await run.check("the two Output buttons are one control with one name, which says what is behind it", async () => {
    // Output side panel PRD §6.2: *Output: N files from this thread* or *Output: nothing
    // produced yet in this thread*, on BOTH buttons, never a bare count — the count numeral is
    // `aria-hidden` and the icon is a glyph, so the name cannot be "Output 9".
    const p = await openApp(browser, { output: "round-17" });
    const read = () =>
      p.evaluate(() =>
        ["out-top", "out-bottom"].map((id) => {
          const b = document.getElementById(id);
          return {
            id,
            name: b.getAttribute("aria-label"),
            countHidden: b.querySelector(".out-count").getAttribute("aria-hidden"),
            glyphHidden: b.querySelector("svg").getAttribute("aria-hidden"),
            controls: b.getAttribute("aria-controls"),
            keys: b.getAttribute("aria-keyshortcuts"),
            title: b.getAttribute("title"),
          };
        })
      );
    const empty = await read();
    for (const b of empty) {
      assertEqual(b.name, "Output: nothing produced yet in this thread", "#" + b.id + " on a thread with no files");
      assertEqual(b.countHidden, "true", "#" + b.id + "'s count is in the name computation");
      assertEqual(b.glyphHidden, "true", "#" + b.id + "'s icon is in the name computation");
      assertEqual(b.controls, "outpanel");
      assert(/Meta\+Shift\+O/.test(b.keys || ""), "#" + b.id + " does not declare its shortcut");
      assert(HAS_A_WORD.test(b.name) && !b.title.includes(b.name), "#" + b.id + " keeps its name in the tooltip");
    }
    await p.click('.nav-thread[data-thread-id="acme"]');
    await p.waitForFunction(() => window.RichOutput.snapshot().count === 9);
    const nine = await read();
    assertEqual(nine.map((b) => b.name), ["Output: 9 files from this thread", "Output: 9 files from this thread"]);
    assertEqual(p.__errors, [], "the page reported errors");
    await p.close();
    return "both: \"" + empty[0].name + "\" → \"" + nine[0].name + "\"; count and icon aria-hidden";
  });

  await run.check("no page errors", async () => {
    assertEqual(page.__errors, [], "the page reported errors");
    return "0 errors";
  });

  await page.close();
  await browser.close();
  // `report()` returns the FAILED COUNT, not a verdict. Same form as every other suite here.
  process.exitCode = run.report() > 0 ? 1 : 0;
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

// THE INPUT AREA, ROUND 51 — richos-hq `design/mockups/rounds/round-51`, the CEO's order of
// 2026-11-01: "implement round-51". His recordings and session notes are in that round's
// README; this suite holds what the app must do so the box stays what he chose.
//
// What it checks, in both themes where a color or a picture is involved:
//
//   1. The empty box: two lines, the placeholder "Type here… OR…" / "← Click to talk to Rich."
//      with the real Talk to Rich button at the start of line two, centered over the plus, and
//      the caret already in the box when a conversation is opened.
//   2. The first character, any character, clears the placeholder and moves the button into
//      the row after the plus; emptying the box brings both back.
//   3. Growth: two lines to ten and a half, then it scrolls and the top line fades.
//   4. The row: Output centered on the box at any width, the status line without its "⋯ ",
//      truncating before Output moves; Send in the corner, filled when there is something to
//      send.
//   5. HIS CORRECTION (2026-11-01): toggling talk moves the button NOT AT ALL, in the empty box
//      and with words in it; the box keeps its height; the words come back when voice ends.
//   6. The plus opens the file picker directly; what is picked lands on the tray; Cancel adds
//      nothing; the caret is back in the box either way.
//   7. Every readable run in the box (placeholder, arrow, listening line, status line) clears
//      4.5:1 on the paint behind it, light and dark, computed.
//
// Run: node input-area.js

"use strict";

const path = require("path");
const { loadPlaywright, leaveHome, createRun, assert, assertEqual, UI_DIR } = require("./lib/harness");
const C = require("./lib/contrast");

const APP = "file://" + path.join(UI_DIR, "index.html");

async function openApp(browser, opts) {
  opts = opts || {};
  const page = await browser.newPage({ viewport: opts.viewport || { width: 1440, height: 900 } });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push("console: " + m.text());
  });
  page.__errors = errors;
  await page.addInitScript((o) => {
    try {
      window.localStorage.setItem("richos-mock-config", JSON.stringify({ theme: o.theme }));
      window.localStorage.setItem("richos-theme", o.theme);
    } catch (e) {
      /* storage unavailable: the shipped default applies */
    }
    // The microphone path the mock offers (`start_voice_capture` answers) so talk can be toggled.
    window.__RICHOS_MOCK_PRESET__ = { bugVoice: true };
  }, { theme: opts.theme || "dark" });
  await page.goto(APP);
  await leaveHome(page);
  await page.waitForSelector(".nav-thread", { state: "attached" });
  await page.click('.nav-thread[data-thread-id="acme"]');
  await page.waitForSelector("#input-shell", { state: "visible" });
  await page.waitForFunction(() => !document.getElementById("input").disabled);
  return page;
}

/// Geometry and state of the box, in one read.
const READ = `(() => {
  const r = (el) => { if (!el) return null; const b = el.getBoundingClientRect(); return { x: b.left, y: b.top, w: b.width, h: b.height, cx: b.left + b.width / 2, cy: b.top + b.height / 2 }; };
  const talk = document.getElementById("talk-toggle");
  const shell = document.getElementById("input-shell");
  const hint = document.getElementById("composer-hint");
  return {
    shell: r(shell),
    input: r(document.getElementById("input")),
    talk: r(talk),
    add: r(document.getElementById("attach-add")),
    out: r(document.getElementById("out-bottom")),
    send: r(document.getElementById("send")),
    slot: talk.parentElement.dataset.slot,
    hintShown: getComputedStyle(hint).visibility !== "hidden",
    hintText: hint.textContent.replace(/\\s+/g, " ").trim(),
    blank: shell.classList.contains("is-blank"),
    hasContent: shell.classList.contains("has-content"),
    voice: !document.getElementById("voice-panel").hidden,
    fieldHidden: getComputedStyle(document.getElementById("composer-field")).visibility === "hidden",
    sendHidden: document.getElementById("send").hidden,
    sendBg: getComputedStyle(document.getElementById("send")).backgroundColor,
    value: document.getElementById("input").value,
    focus: document.activeElement && document.activeElement.id,
    talks: document.querySelectorAll("#talk-toggle").length,
    // Talk to Rich must stay in the accessibility tree wherever it sits: no hidden ancestor.
    talkHidden: !!talk.closest('[aria-hidden="true"], [hidden]'),
    fadeTop: document.getElementById("input").classList.contains("fade-top"),
    lineH: parseFloat(getComputedStyle(document.getElementById("input")).lineHeight),
  };
})()`;

async function read(page) {
  return page.evaluate(READ);
}

/// Wait for the FACT, never the clock: two frames for layout and the fade's own frame, then every
/// finite animation in the box (Talk to Rich's glide, the placeholder's fade-in, the field's
/// height ease) to its end. The loops that never end (the pressed icon's bars, the listening dot,
/// the level meter) are not waited for.
async function settle(page) {
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
  await page.evaluate(() =>
    Promise.all(
      document
        .getAnimations()
        .filter((a) => a.effect && a.effect.target && a.effect.target.closest && a.effect.target.closest("#composer-zone"))
        .filter((a) => a.effect.getComputedTiming().endTime !== Infinity)
        .map((a) => a.finished.catch(() => {}))
    )
  );
}

async function setValue(page, text) {
  await page.evaluate((t) => {
    const i = document.getElementById("input");
    i.value = t;
    i.dispatchEvent(new Event("input", { bubbles: true }));
  }, text);
  await settle(page);
}

const near = (a, b, tol) => Math.abs(a - b) <= (tol == null ? 0.5 : tol);

/// The ratio of every visible text run inside `selector`, on the paint stack behind it, with the
/// shipped arithmetic (`window.__contrastMath`, which `C.pageScript()` exposes).
const MEASURE_TEXT = `((selector) => {
  const M = window.__contrastMath;
  const out = [];
  for (const root of document.querySelectorAll(selector)) {
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    for (let n = walker.nextNode(); n; n = walker.nextNode()) {
      const text = n.textContent.trim();
      if (!text) continue;
      const host = n.parentElement;
      const range = document.createRange();
      range.selectNodeContents(n);
      const rects = [...range.getClientRects()].filter((b) => b.width > 0 && b.height > 0);
      if (!rects.length) continue;
      const b = rects[0];
      const stack = document.elementsFromPoint(b.left + Math.min(4, b.width / 2), b.top + b.height / 2);
      const layers = [];
      for (const el of stack) {
        const c = M.parseCssColor(getComputedStyle(el).backgroundColor);
        if (!c || c.a === 0) continue;
        layers.push(c);
        if (c.a >= 1) break;
      }
      if (!layers.length) { out.push({ text, error: "nothing painted under it" }); continue; }
      let ground = layers[layers.length - 1];
      for (let k = layers.length - 2; k >= 0; k--) ground = M.compositeOver(layers[k], ground);
      const fg = M.parseCssColor(getComputedStyle(host).color);
      const ink = fg.a < 1 ? M.compositeOver(fg, ground) : fg;
      const px = parseFloat(getComputedStyle(host).fontSize);
      out.push({ text, ratio: M.round2(M.contrastRatio(ink, ground)), px, ink: M.hex(ink), ground: M.hex(ground) });
    }
  }
  return out;
})`;

async function measure(page, selector) {
  await page.evaluate(C.pageScript());
  return page.evaluate(`${MEASURE_TEXT}(${JSON.stringify(selector)})`);
}

async function main() {
  const pw = loadPlaywright();
  const run = createRun("the input area, round 51");
  const browser = await pw.webkit.launch();

  // ---- 1. the empty box ---------------------------------------------------------------------
  for (const theme of ["dark", "light"]) {
    await run.check(`1. ${theme}: the empty box is two lines, the placeholder holds Talk to Rich over the plus, the caret is in it`, async () => {
      const page = await openApp(browser, { theme });
      const s = await read(page);
      assertEqual(s.value, "", "the box did not open empty");
      assertEqual(s.focus, "input", "the caret is not in the box after opening a conversation");
      assert(s.hintShown && s.blank, "the placeholder is not drawn in the empty box");
      assertEqual(s.hintText, "Type here… OR… ← Click to talk to Rich.", "the placeholder's words");
      assertEqual(s.slot, "hint", "Talk to Rich is not at the start of the placeholder's second line");
      assertEqual(s.talks, 1, "there must be exactly one Talk to Rich button");
      assert(!s.talkHidden, "Talk to Rich sits inside an aria-hidden or hidden ancestor in the empty box: out of the accessibility tree");
      assert(near(s.input.h, s.lineH * 2), `the empty field is not two lines: ${s.input.h}px for ${s.lineH}px lines`);
      assert(near(s.talk.cx, s.add.cx), `Talk to Rich (${s.talk.cx}) is not centered over the plus (${s.add.cx})`);
      assert(s.talk.y < s.add.y, "Talk to Rich is not above the plus");
      assertEqual(s.talk.w, 36, "Talk to Rich is not the row's 36px size in the placeholder");
      assert(page.__errors.length === 0, "page errors: " + page.__errors.join(" | "));
      await page.close();
      return `field ${s.input.h}px, talk center ${s.talk.cx} over plus ${s.add.cx}`;
    });
  }

  // ---- 2. the first character, and back ------------------------------------------------------
  await run.check("2. the first character, a space included, clears the placeholder and moves Talk to Rich into the row; emptying brings it back", async () => {
    const page = await openApp(browser, { theme: "dark" });
    const before = await read(page);
    await page.keyboard.type(" ");
    await settle(page);
    const typed = await read(page);
    assertEqual(typed.value, " ", "the space did not land in the box (the caret was not there)");
    assert(!typed.hintShown, "the placeholder is still drawn over a typed character");
    assertEqual(typed.slot, "row", "Talk to Rich did not move into the row");
    assert(typed.talk.x > typed.add.x + 30 && near(typed.talk.cy, typed.add.cy), "Talk to Rich is not right after the plus, on its line");
    assert(near(typed.shell.h, before.shell.h), `the box changed height on the first character: ${before.shell.h} -> ${typed.shell.h}`);
    await page.keyboard.press("Backspace");
    await settle(page);
    const back = await read(page);
    assertEqual(back.slot, "hint", "Talk to Rich did not come back up when the box was emptied");
    assert(back.hintShown, "the placeholder did not come back");
    assert(near(back.talk.x, before.talk.x) && near(back.talk.y, before.talk.y), "Talk to Rich did not return to the same place");
    await page.close();
    return `hint (${before.talk.x},${before.talk.y}) -> row (${typed.talk.x},${typed.talk.y}) -> hint`;
  });

  // ---- 3. growth -------------------------------------------------------------------------------
  await run.check("3. the field grows one line at a time to ten and a half lines, then scrolls with the top line faded", async () => {
    const page = await openApp(browser, { theme: "dark" });
    const lh = (await read(page)).lineH;
    await setValue(page, "one\ntwo\nthree\nfour");
    const four = await read(page);
    assert(near(four.input.h, lh * 4), `four lines are not four lines tall: ${four.input.h}`);
    await setValue(page, Array.from({ length: 20 }, (_, i) => "line " + (i + 1)).join("\n"));
    await page.evaluate(() => { const i = document.getElementById("input"); i.scrollTop = i.scrollHeight; i.dispatchEvent(new Event("scroll")); });
    await settle(page);
    const long = await read(page);
    assert(near(long.input.h, lh * 10.5), `twenty lines are not capped at ten and a half: ${long.input.h} vs ${lh * 10.5}`);
    assert(long.fadeTop, "scrolled to the end, the top line does not fade");
    await page.close();
    return `4 lines ${four.input.h}px; 20 lines capped at ${long.input.h}px, top faded`;
  });

  await run.check("3b. a narrower window rewraps the words and the field grows with them, never hiding a line", async () => {
    // Mid-job review rv-20261101T215812Z-f1f1fca9-a259: no keystroke happens on a resize, so the
    // field is measured again when the box's width changes.
    const page = await openApp(browser, { theme: "dark" });
    const words = Array.from({ length: 4 }, () => "Pull the signed term sheet and the Q3 numbers and check the counter letter twice").join(" ");
    await setValue(page, words);
    const wide = await read(page);
    await page.setViewportSize({ width: 1024, height: 900 });
    await page.waitForFunction(() => {
      const i = document.getElementById("input");
      return i.scrollHeight <= i.clientHeight + 1 || getComputedStyle(i).overflowY === "auto";
    });
    await settle(page);
    const narrow = await page.evaluate(() => {
      const i = document.getElementById("input");
      return { h: i.getBoundingClientRect().height, scroll: i.scrollHeight, client: i.clientHeight, overflow: getComputedStyle(i).overflowY };
    });
    assert(narrow.h > wide.input.h, `the field did not grow when its words rewrapped: ${wide.input.h}px -> ${narrow.h}px`);
    assert(narrow.scroll <= narrow.client + 1, `a line is hidden below the field: scrollHeight ${narrow.scroll} > ${narrow.client} (overflow ${narrow.overflow})`);
    await page.close();
    return `${wide.input.h}px at 1440 -> ${narrow.h}px at 1024, nothing hidden`;
  });

  // ---- 4. the row ----------------------------------------------------------------------------
  for (const width of [1440, 1100]) {
    await run.check(`4. at ${width}px: Output is centered on the box, the status line has no "⋯ ", Send is in the corner`, async () => {
      const page = await openApp(browser, { theme: "dark", viewport: { width, height: 900 } });
      await page.evaluate(() => { const o = document.getElementById("out-bottom"); o.hidden = false; });
      await settle(page);
      const s = await read(page);
      assert(near(s.out.cx, s.shell.cx, 1), `Output's center ${s.out.cx} is not the box's ${s.shell.cx}`);
      const chip = await page.evaluate(() => { const c = document.querySelector("#drill-chip-zone .drill-chip"); return c ? c.textContent : null; });
      assert(chip !== null, "the mock thread shows no status line to check");
      assert(!chip.startsWith("⋯"), `the status line still starts with the ellipsis: ${JSON.stringify(chip)}`);
      assert(near(s.send.x + s.send.w, s.shell.x + s.shell.w - 9, 1.5), "Send is not in the box's right corner");
      await page.close();
      return `Output ${s.out.cx} / box ${s.shell.cx}; status ${JSON.stringify(chip)}`;
    });
  }

  await run.check("4b. Send fills with the accent once there is something to send", async () => {
    const page = await openApp(browser, { theme: "light" });
    const rest = await read(page);
    await setValue(page, "Draft the reply");
    const ready = await read(page);
    assert(!rest.hasContent && ready.hasContent, "the box does not say it has something to send");
    assert(rest.sendBg !== ready.sendBg, `Send looks the same with and without words: ${rest.sendBg}`);
    await page.close();
    return `Send ${rest.sendBg} -> ${ready.sendBg}`;
  });

  // ---- 5. his correction: talk never moves the button ------------------------------------------
  for (const words of ["", "Draft the reply to Acme\nat list minus 3%\nand keep it firm"]) {
    const label = words ? "with three lines in the box" : "in the empty box";
    await run.check(`5. ${label}: starting and stopping talk moves Talk to Rich not at all, and the box keeps its height`, async () => {
      const page = await openApp(browser, { theme: "dark" });
      if (words) await setValue(page, words);
      const before = await read(page);
      await page.click("#talk-toggle");
      await page.waitForFunction(() => document.getElementById("talk-toggle").getAttribute("aria-pressed") === "true");
      // Read at once (a jump) and again once every animation in the box has ended (a glide).
      const during = await read(page);
      await settle(page);
      const talking = await read(page);
      assert(talking.voice && talking.fieldHidden, "the box did not become the listening panel");
      assert(talking.sendHidden, "Send is still offered while talking, with nothing typed to send");
      assert(!talking.talkHidden, "Talk to Rich is inside an aria-hidden or hidden ancestor while talking");
      for (const [when, s] of [["at once", during], ["after 400ms", talking]]) {
        assert(near(s.talk.x, before.talk.x) && near(s.talk.y, before.talk.y), `Talk to Rich moved when talk started (${when}): (${before.talk.x},${before.talk.y}) -> (${s.talk.x},${s.talk.y})`);
      }
      assert(near(talking.shell.h, before.shell.h) && near(talking.shell.y, before.shell.y), `the box changed when talk started: ${before.shell.h}px -> ${talking.shell.h}px`);
      // A glide is a script animation (`Element.animate`) on the button itself; the pressed icon's
      // breathing bars are CSS animations on its paths and are not movement.
      const animating = await page.evaluate(() => document.getElementById("talk-toggle").getAnimations().filter((a) => a.constructor.name === "Animation").length);
      assertEqual(animating, 0, "a movement animation is running on Talk to Rich after talk started");
      await page.click("#talk-toggle");
      await page.waitForFunction(() => document.getElementById("talk-toggle").getAttribute("aria-pressed") === "false");
      const stopped0 = await read(page);
      await settle(page);
      const stopped = await read(page);
      for (const [when, s] of [["at once", stopped0], ["after 400ms", stopped]]) {
        assert(near(s.talk.x, before.talk.x) && near(s.talk.y, before.talk.y), `Talk to Rich moved when talk stopped (${when})`);
      }
      assertEqual(stopped.value, words, "the words in the box did not come back when voice ended");
      assert(!stopped.voice && !stopped.fieldHidden, "the text area did not come back");
      assertEqual(stopped.focus, "input", "the caret is not back in the box when voice ends");
      assert(page.__errors.length === 0, "page errors: " + page.__errors.join(" | "));
      await page.close();
      return `Talk to Rich at (${before.talk.x},${before.talk.y}) before, during and after; box ${before.shell.h}px throughout`;
    });
  }

  // ---- 6. the plus ---------------------------------------------------------------------------
  await run.check("6. the plus opens the file picker; picked files land on the tray; Cancel adds nothing; the caret comes back", async () => {
    const page = await openApp(browser, { theme: "dark" });
    await page.evaluate(() => window.__RICHOS_MOCK__.pickFiles(null));
    await page.click("#attach-add");
    // The picker answered (Cancel) and the plus's handler gave the caret back.
    await page.waitForFunction(() =>
      window.__RICHOS_MOCK__.attachCalls().some((c) => c.cmd === "attach_pick_files") && document.activeElement.id === "input"
    );
    let chips = await page.$$eval("#attach-list .attach-chip", (n) => n.length);
    assertEqual(chips, 0, "Cancel put something on the tray");
    assertEqual((await read(page)).focus, "input", "the caret is not back in the box after Cancel");
    await page.evaluate(() =>
      window.__RICHOS_MOCK__.pickFiles([
        { name: "brief.pdf", bytes: [0x25, 0x50, 0x44, 0x46, 0x2d, 0x31] },
        { name: "chart.png", bytes: [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a] },
      ])
    );
    await page.click("#attach-add");
    await page.waitForFunction(() => document.querySelectorAll("#attach-list .attach-chip").length === 2);
    const calls = await page.evaluate(() => window.__RICHOS_MOCK__.attachCalls().map((c) => c.cmd));
    assert(calls.filter((c) => c === "attach_pick_files").length === 2, "the plus did not ask for the picker: " + calls.join(","));
    assert(calls.filter((c) => c === "attach_dropped_file").length === 2, "the picked files did not take the drop's road: " + calls.join(","));
    const s = await read(page);
    assertEqual(s.focus, "input", "the caret is not back in the box after picking");
    assert(s.hasContent, "files without words are not something to send");
    assertEqual(s.slot, "hint", "files without words moved Talk to Rich out of the placeholder (only typing does)");
    await page.close();
    return calls.join(" → ");
  });

  // ---- 7. contrast, both themes ----------------------------------------------------------------
  for (const theme of ["dark", "light"]) {
    await run.check(`7. ${theme}: every readable run in the box clears 4.5:1, computed (placeholder, arrow, status line, listening line)`, async () => {
      const page = await openApp(browser, { theme });
      const rows = await measure(page, "#composer-hint, .composer-status");
      await page.click("#talk-toggle");
      await page.waitForSelector("#voice-panel:not([hidden])");
      await settle(page);
      const talking = await measure(page, "#voice-state-listening, .voice-talk-line");
      const all = rows.concat(talking);
      const bad = all.filter((r) => r.error || r.ratio < 4.5 || r.px < 16);
      assert(all.length >= 5, "too few runs measured to prove anything: " + JSON.stringify(all));
      assert(!bad.length, "below the floor: " + JSON.stringify(bad));
      await page.close();
      return all.map((r) => `${JSON.stringify(r.text.slice(0, 22))} ${r.ratio}:1`).join("; ");
    });
  }

  await browser.close();
  // `report()` returns the FAILED COUNT, not a verdict.
  process.exitCode = run.report() > 0 ? 1 : 0;
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

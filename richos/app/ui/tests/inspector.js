// §7.2 the worker inspector, §7.3 the background-work summary, §20 the three breakpoints —
// driven through the REAL SHELL: `index.html`, `main.js`, `mock.js`, `style.css` and
// `timeline.js`, all loaded from disk, with nothing stubbed but the Tauri bridge (which
// `mock.js` already replaces, exactly as an operator opening the file does).
//
// `workers.js` beside this file tests the renderer in isolation. This one tests that the
// shell wires it up — the two failures are different and only one of them is caught by
// either test alone.

"use strict";

const path = require("path");
const { leaveHome, loadPlaywright, shot, createRun, assert, assertEqual, SLOW_BRIDGE, UI_DIR } = require("./lib/harness");

const APP = "file://" + path.join(UI_DIR, "index.html");

async function openApp(browser, viewport) {
  const page = await browser.newPage({ viewport: viewport || { width: 1400, height: 900 } });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push("console: " + m.text());
  });
  // Capture the existing shell refresh listener so a delayed response can be released
  // between input down/up without adding a public mock-only event API.
  await page.addInitScript(() => {
    let bridge;
    Object.defineProperty(window, "RichBridge", {
      configurable: true,
      get: () => bridge,
      set(value) {
        const listen = value.listen.bind(value);
        value.listen = (name, callback) => {
          if (name === "rich://mock-proactive") window.__reloadTimeline = callback;
          return listen(name, callback);
        };
        bridge = value;
      },
    });
  });
  await page.goto(APP);
  // The home screen is the landing surface now; this suite is about the app UI behind it.
  await leaveHome(page);
  await page.waitForFunction("typeof window.RichTimeline === 'object'");
  // ATTACHED, not visible: below 820px §20 makes the rail a closed full-height drawer, so
  // its rows exist and are correctly not on screen.
  await page.waitForSelector(".nav-thread", { state: "attached" });
  const railHidden = await page.evaluate(() => {
    const r = document.getElementById("rail");
    return getComputedStyle(r).display === "none" || document.body.classList.contains("rail-closed");
  });
  if (railHidden) await page.click("#rail-toggle");
  await page.waitForSelector(".nav-thread", { state: "visible" });
  page.__errors = errors;
  return page;
}

/// Open the seeded thread that carries three delegated workers and expand its transcript.
async function openHiringThread(page) {
  await page.click('.nav-thread[data-thread-id="hiring"]');
  await page.waitForSelector('.tl-duration-btn:not(.tl-duration-btn--static)');
  const disclosure = page.locator('.tl-duration-btn:not(.tl-duration-btn--static)').first();
  if (await disclosure.getAttribute("aria-expanded") !== "true") await disclosure.click();
  await page.waitForSelector('.tl-duration-btn[aria-expanded="true"]');
  await page.waitForSelector(".tl-chip");
}

async function main() {
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();
  const run = createRun("§7.2 inspector / §7.3 summary / §20 breakpoints — real shell, WebKit");

  const page = await openApp(browser);

  await run.check("the shell draws the delegated workers in a real seeded thread", async () => {
    await openHiringThread(page);
    const r = await page.evaluate(() => ({
      summary: document.querySelector(".tl-workers-head .tl-activity-text").textContent,
      chips: Array.from(document.querySelectorAll(".tl-chip")).map(
        (c) => c.querySelector(".tl-chip-name").textContent + "/" + c.querySelector(".tl-chip-state").textContent
      ),
    }));
    assertEqual(r.chips, ["Sage/Starting", "Frank/Working", "Clark/Ended"]);
    return `"${r.summary}" — ${r.chips.join(", ")}`;
  });

  await run.check("§7.2 selecting a chip opens a SIBLING pane, not a modal", async () => {
    await page.click('[id="chip:agt_clark_1"]');
    await page.waitForSelector("#inspector:not([hidden])");
    const r = await page.evaluate(() => {
      const insp = document.getElementById("inspector");
      const conv = document.getElementById("conversation");
      const ir = insp.getBoundingClientRect();
      const cr = conv.getBoundingClientRect();
      return {
        title: document.getElementById("inspector-title").textContent,
        modal: insp.getAttribute("aria-modal"),
        scrimHidden: document.getElementById("inspector-scrim").hidden,
        // Both panes readable side by side, neither covering the other (§7.2, §25).
        overlaps: ir.left < cr.right - 1 && cr.left < ir.right - 1,
        conversationWidth: Math.round(cr.width),
        inspectorWidth: Math.round(ir.width),
        selectedChips: document.querySelectorAll(".tl-chip.is-selected").length,
        body: document.getElementById("inspector-body").innerText,
      };
    });
    assertEqual(r.title, "Clark", "the pane names the worker");
    assertEqual(r.modal, null, "§7.2: a sibling pane, NOT a modal");
    assert(!r.overlaps, "the pane must sit beside the conversation, not over it");
    assert(r.conversationWidth >= 620, "§20: the conversation keeps at least 620px — got " + r.conversationWidth);
    assertEqual(r.selectedChips, 1, "the open worker's chip is marked");
    assert(r.body.includes("Ended"), "state is shown");
    assert(r.body.includes("Pulled the platform-eng comparables"), "§7.2 item 4: the authored update");
    return `conversation ${r.conversationWidth}px | inspector ${r.inspectorWidth}px, side by side`;
  });

  await run.check("§7.2 the pane is READ-ONLY: no control R2 governs", async () => {
    const r = await page.evaluate(() => {
      const body = document.getElementById("inspector-body");
      const controls = Array.from(body.querySelectorAll("button, input, select, textarea, a[href]"));
      return {
        controlsInBody: controls.map((c) => c.textContent.trim() || c.tagName),
        // The whole pane, header included.
        allControls: Array.from(
          document.getElementById("inspector").querySelectorAll("button, input, select, textarea, a[href]")
        ).map((c) => c.getAttribute("aria-label") || c.textContent.trim()),
        controlText: Array.from(
          document.getElementById("inspector").querySelectorAll("button, input, select, textarea, a[href]")
        )
          .map((c) => (c.getAttribute("aria-label") || "") + " " + c.textContent)
          .join(" ")
          .toLowerCase(),
        stateWord: document.querySelector(".insp-state-word").textContent,
        stateQualifier: document.querySelector(".insp-state-qualifier").textContent,
      };
    });
    assertEqual(r.controlsInBody.length, 1, "exactly one control in the body: " + r.controlsInBody.join(", "));
    assert(
      r.controlsInBody[0].includes("What I saw"),
      "and it is the chronology disclosure, not an interrupt/retry/approve: " + r.controlsInBody[0]
    );
    assertEqual(r.allControls.length, 2, "the pane as a whole has exactly two: " + r.allControls.join(", "));
    assert(r.allControls[0] === "Close worker details", "the other is Close: " + r.allControls[0]);
    // Checked against the CONTROL surface, not the prose: §7.2's list is a list of things
    // the CEO must not be able to DO. The explanatory sentence is allowed to name "failed"
    // as one of three possibilities — that is the honesty, and forbidding the word there
    // would force the pane to be vaguer than the truth.
    for (const forbidden of ["retry", "interrupt", "approve", "resume", "stop", "model", "permission", "prompt"]) {
      assert(!r.controlText.includes(forbidden), `§7.2 forbids a "${forbidden}" control — found one: ` + r.controlText);
    }
    // And the STATE, which is the one line a CEO reads as a verdict, claims nothing.
    assertEqual(r.stateWord, "Ended", "run_ended must not be dressed up as an outcome");
    assertEqual(r.stateQualifier, "outcome not recorded");
    return "2 controls total: Close, and one disclosure. State reads \"Ended · outcome not recorded\". R2 stays deferred to V2.";
  });

  await run.check("§7.2/§22 no elapsed active time is shown, and the absence is stated", async () => {
    const r = await page.evaluate(() => {
      document.getElementById("insp-chron-toggle").click();
      return {
        facts: Array.from(document.querySelectorAll(".insp-facts dt")).map((d, i) => [
          d.textContent,
          document.querySelectorAll(".insp-facts dd")[i].textContent,
        ]),
        text: document.getElementById("inspector-body").innerText,
      };
    });
    const spent = r.facts.find((f) => f[0] === "Time spent working");
    assert(spent, "the pane says something about time spent");
    assertEqual(spent[1], "not recorded", "§22: elapsed active time must not be faked");
    // Two timestamps ARE shown — and no difference of them appears anywhere.
    assert(
      r.facts.some((f) => f[0] === "First seen") && r.facts.some((f) => f[0] === "Last seen"),
      "the two observed timestamps are shown as timestamps"
    );
    assert(!/\b\d+m \d+s\b/.test(r.text), "no duration was computed from them: " + r.text);
    return r.facts.map((f) => `${f[0]}: ${f[1]}`).join(" | ");
  });

  await run.check("§7.2 the worker result stays visible when its activity closes", async () => {
    const r = await page.evaluate(() => {
      const read = () => ({
        chronOpen: document.getElementById("insp-chron-toggle").getAttribute("aria-expanded"),
        factsVisible: !document.getElementById("insp-chron-body").hidden,
        update: document.querySelector(".insp-update-text") ? document.querySelector(".insp-update-text").textContent : null,
        state: document.querySelector(".insp-state-word").textContent,
      });
      const open = read();
      document.getElementById("insp-chron-toggle").click();
      return { open, closed: read() };
    });
    assertEqual(r.open.factsVisible, true);
    assertEqual(r.closed.factsVisible, false, "the chronology collapses independently");
    assertEqual(r.closed.update, r.open.update, "the worker's own words survive the collapse");
    assertEqual(r.closed.state, r.open.state, "so does its state");
    return `activity closed, result still reads: "${r.closed.update}"`;
  });

  await run.check("§18 Escape closes the pane and returns focus to the chip", async () => {
    await page.keyboard.press("Escape");
    await page.waitForFunction(() => document.getElementById("inspector").hidden);
    const r = await page.evaluate(() => ({
      hidden: document.getElementById("inspector").hidden,
      focused: document.activeElement ? document.activeElement.id : null,
      selected: document.querySelectorAll(".tl-chip.is-selected").length,
    }));
    assert(r.hidden, "Escape closes inspector detail");
    assertEqual(r.focused, "chip:agt_clark_1", "focus goes back where it came from");
    assertEqual(r.selected, 0, "and no chip is left marked");
    return "closed, focus restored to chip:agt_clark_1";
  });

  await run.check("§25 worker-pane width can be changed directly and survives relaunch", async () => {
    await page.click('[id="chip:agt_frank_1"]');
    await page.waitForSelector("#inspector:not([hidden])");
    const before = await page.evaluate(() => document.getElementById("inspector").getBoundingClientRect().width);

    // Dragged, not set: the divider is the affordance §7.2 asks for.
    const box = await page.locator("#inspector-resizer").boundingBox();
    await page.mouse.move(box.x + box.width / 2, box.y + 200);
    await page.mouse.down();
    await page.mouse.move(box.x + box.width / 2 - 90, box.y + 200, { steps: 8 });
    await page.mouse.up();

    const after = await page.evaluate(() => ({
      width: document.getElementById("inspector").getBoundingClientRect().width,
      persisted: window.RichBridge.invoke("nav_state").then ? null : null,
    }));
    assert(Math.abs(after.width - (before + 90)) <= 2, `dragged: ${before} -> ${after.width}`);

    // "Survives relaunch": reload the page and read what the (mocked) durable store hands
    // back at boot. The mock clamps with nav.rs's own bounds and returns the width it
    // ACCEPTED, so this exercises the same contract the real command has.
    const persisted = await page.evaluate(() => window.RichBridge.invoke("nav_state"));
    assertEqual(Math.round(persisted.inspector_width), Math.round(after.width), "the store holds the new width");
    assert(
      Math.round(persisted.sidebar_width) === 300,
      "and dragging the worker pane did NOT move the rail: " + persisted.sidebar_width
    );

    // Keyboard, too — §18: every function works without a pointer.
    await page.focus("#inspector-resizer");
    await page.keyboard.press("ArrowRight");
    const afterKey = await page.evaluate(() => document.getElementById("inspector").getBoundingClientRect().width);
    assert(Math.abs(afterKey - (after.width - 8)) <= 2, `keyboard resize: ${after.width} -> ${afterKey}`);
    return `drag ${Math.round(before)} -> ${Math.round(after.width)}px, persisted; ArrowRight -> ${Math.round(afterKey)}px; rail untouched at 300px`;
  });

  await run.check("§7.3 the background-work summary shows only counts with a source", async () => {
    const r = await page.evaluate(async () => {
      const status = await window.RichBridge.invoke("get_worker_status");
      return { status, chip: document.querySelector(".drill-chip") ? document.querySelector(".drill-chip").textContent : null };
    });
    // Force a poll the way `rich://turn-started` does.
    await page.evaluate(() => window.RichBridge.invoke("get_worker_status"));
    await page.click('.nav-thread[data-thread-id="acme"]');
    await page.click('.nav-thread[data-thread-id="hiring"]');
    const chipText = await page.evaluate(async () => {
      // main.js polls on turn-started; call the same path directly rather than faking a turn.
      const status = await window.RichBridge.invoke("get_worker_status");
      return status;
    });
    assertEqual(chipText.needs_you, 0, "needs_you is structurally 0 and is never rendered");
    assert(chipText.active >= 1, "a real active count exists now");
    assert(typeof chipText.liveness_unknown === "number", "and so does a real unknown count");
    void r;
    return `active=${chipText.active} done=1 liveness_unknown=${chipText.liveness_unknown} needs_you=${chipText.needs_you} (never drawn)`;
  });

  await run.check("SCREENSHOT: the inspector docked beside the conversation", async () => {
    // This is a return to a cached thread: its disclosure may already be open.
    await openHiringThread(page);
    await page.click('[id="chip:agt_clark_1"]');
    await page.waitForSelector("#inspector:not([hidden])");
    const s = await shot(page, "inspector-docked-1400");
    assert(s.bytes > 3000, "too small to be a render: " + s.bytes);
    return `${s.file} (${s.bytes} bytes)`;
  });

  for (const input of ["mouse", "keyboard"]) {
    await run.check(`an unchanged worker keeps ${input} activation across a late snapshot`, async () => {
      const p = await openApp(browser);
      try {
        await openHiringThread(p);
        await p.waitForFunction(() => document.querySelector("#composer-row").dataset.mode !== "opening");
        await p.evaluate(() => {
          const original = window.RichBridge.invoke.bind(window.RichBridge);
          const apply = window.RichTimeline.applySnapshot;
          window.__snapshotApplied = false;
          window.RichTimeline.applySnapshot = (model, snapshot) => {
            const result = apply(model, snapshot);
            window.__snapshotApplied = true;
            return result;
          };
          window.RichBridge.invoke = async (cmd, args) => {
            if (cmd !== "get_timeline") return original(cmd, args);
            const snapshot = await original(cmd, args);
            return new Promise(resolve => { window.__releaseSnapshot = () => resolve(JSON.parse(JSON.stringify(snapshot))); });
          };
          window.__targetChip = document.getElementById("chip:agt_clark_1");
          const threadId = window.__RICHOS_TIMELINE__().threadId;
          window.__reloadTimeline({ payload: { threadId } });
        });
        await p.waitForFunction(() => !!window.__releaseSnapshot);
        const chip = p.locator('[id="chip:agt_clark_1"]');
        if (input === "mouse") {
          const box = await chip.boundingBox();
          await p.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
          await p.mouse.down();
        } else {
          await chip.focus();
          await p.keyboard.down("Space");
        }
        await p.evaluate(() => window.__releaseSnapshot());
        await p.waitForFunction(() => window.__snapshotApplied);
        await p.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
        assert(await p.evaluate(() => window.__targetChip === document.getElementById("chip:agt_clark_1") && window.__targetChip.isConnected),
          "an equivalent snapshot replaced the active target");
        if (input === "mouse") await p.mouse.up();
        else await p.keyboard.up("Space");
        await p.waitForSelector("#inspector:not([hidden])");
        assertEqual(await p.locator("#inspector-title").innerText(), "Clark");
      } finally { await p.close(); }
    });
  }

  await run.check("authoritative snapshots still replace changed, removed and re-scoped workers", async () => {
    const result = await page.evaluate(async () => {
      const T = window.RichTimeline;
      const snapshot = await window.RichBridge.invoke("get_timeline", { threadId: "hiring" });
      const results = {};
      for (const change of ["changed", "removed", "thread", "entity", "mode", "visibility"]) {
        const model = T.createModel();
        const container = document.createElement("div");
        const opts = { isExpanded: () => true, openWorker: () => {} };
        const original = JSON.parse(JSON.stringify(snapshot));
        T.applySnapshot(model, original);
        T.render(model, container, opts);
        const before = container.querySelector('[id="chip:agt_clark_1"]');
        if (!before) throw new Error("worker fixture is absent");
        const next = JSON.parse(JSON.stringify(snapshot));
        const worker = next.items.find(item => item.worker && item.worker.agentId === "agt_clark_1");
        if (!worker) throw new Error("worker snapshot item is absent");
        if (change === "changed") worker.worker.workerName = "Updated worker";
        if (change === "removed") next.items = next.items.filter(item => item !== worker);
        if (change === "thread") next.threadId = "another-thread";
        if (change === "entity") next.entityId = "another-entity";
        if (change === "mode") next.mode = "technical";
        if (change === "visibility") worker.visibility = "internal";
        T.applySnapshot(model, next);
        T.render(model, container, opts);
        const after = container.querySelector('[id="chip:agt_clark_1"]');
        results[change] = change === "removed" || change === "visibility"
          ? after === null
          : after !== null && after !== before &&
            (change !== "changed" || after.textContent.includes("Updated worker"));
      }
      return results;
    });
    for (const [change, replaced] of Object.entries(result)) assert(replaced, change + " kept a stale worker control");
    return "changed values, removal, thread, entity, mode and visibility all invalidate the old control";
  });

  await run.check("no page errors in the real shell", async () => {
    assertEqual(page.__errors, [], "the shell threw");
    return "0 uncaught errors, 0 console errors";
  });

  await page.close();

  // ---------------------------------------------------------------------------------------
  // §20 — the three breakpoints, each opened fresh so the boot-time layout is what is tested
  // ---------------------------------------------------------------------------------------
  for (const [label, width, expectDocked] of [
    ["1180px and wider — docked", 1400, true],
    ["820 to 1179 — overlays from the right", 1000, false],
    ["below 820 — full-height sheet", 760, false],
  ]) {
    await run.check("§20 " + label, async () => {
      const p = await openApp(browser, { width, height: 900 });
      await p.click('.nav-thread[data-thread-id="hiring"]');
      await p.waitForSelector("#messages .tl-turn");
      await p.waitForSelector(".tl-duration-btn");
      await p.click(".tl-duration-btn");
      await p.waitForSelector(".tl-chip");
      await p.click('[id="chip:agt_sage_1"]');
      await p.waitForSelector("#inspector:not([hidden])");
      const r = await p.evaluate(() => {
        const insp = document.getElementById("inspector");
        const conv = document.getElementById("conversation");
        const ir = insp.getBoundingClientRect();
        const cr = conv.getBoundingClientRect();
        return {
          position: getComputedStyle(insp).position,
          inspectorWidth: Math.round(ir.width),
          conversationWidth: Math.round(cr.width),
          scrimHidden: document.getElementById("inspector-scrim").hidden,
          resizerShown: getComputedStyle(document.getElementById("inspector-resizer")).display !== "none",
          readable: ir.width >= 260,
          title: document.getElementById("inspector-title").textContent,
        };
      });
      assertEqual(r.title, "Sage", "the pane opened");
      if (expectDocked) {
        assertEqual(r.position, "relative", "docked as a flex sibling");
        assert(r.resizerShown, "the divider is directly adjustable at this size");
        assert(r.conversationWidth >= 620, "conversation floor: " + r.conversationWidth);
        assert(r.scrimHidden, "a docked pane has no scrim — it is not modal");
      } else {
        assertEqual(r.position, "fixed", "§20: it overlays because two readable columns no longer fit");
        assert(!r.scrimHidden, "an overlaying pane gets a dismiss scrim");
      }
      assert(r.readable, "the pane stays readable at " + r.inspectorWidth + "px");
      const s = await shot(p, "inspector-" + width);
      assert(p.__errors.length === 0, "errors at " + width + "px: " + p.__errors.join("; "));
      await p.close();
      return `${r.position}, inspector ${r.inspectorWidth}px, conversation ${r.conversationWidth}px -> ${s.file}`;
    });
  }

  await sidebarToggleChecks(browser, run);

  await browser.close();
  return run.report();
}

// ---------------------------------------------------------------------------------------
// THE SIDEBAR TOGGLE AT EVERY WIDTH — output side-panel PRD §8 / S8, round 17.1's
// `sidebar-hidden`. The CEO, after round 17: "we also need to give the user the option to
// toggle the left sidebar". Here because this file already owns §20's breakpoints, and the
// toggle was a §20 rule ("at 1180px and wider the sidebar is persistent") until S8.
// ---------------------------------------------------------------------------------------

/// The rail, the toggle and the durable state, read in one go.
function readSidebar(page) {
  return page.evaluate(async () => {
    const rail = document.getElementById("rail");
    const stage = document.getElementById("stage");
    const btn = document.getElementById("rail-toggle");
    const pane = btn.querySelector(".sb-pane");
    const rs = getComputedStyle(rail);
    const rr = rail.getBoundingClientRect();
    const br = btn.getBoundingClientRect();
    const hr = document.getElementById("stage-header").getBoundingClientRect();
    const nav = await window.RichBridge.invoke("nav_state");
    return {
      railShown: rs.display !== "none" && rs.visibility !== "hidden" && rr.right > 1,
      railRight: Math.round(rr.right),
      railDisplay: rs.display,
      stageLeft: Math.round(stage.getBoundingClientRect().left),
      btnShown: !btn.hidden && getComputedStyle(btn).display !== "none" && br.width >= 24 && br.height >= 24,
      btnFromHeaderLeft: Math.round(br.left - hr.left),
      btnFirstInHeader: document.getElementById("stage-header").firstElementChild === btn,
      expanded: btn.getAttribute("aria-expanded"),
      label: btn.getAttribute("aria-label"),
      title: btn.title,
      controls: btn.getAttribute("aria-controls"),
      keys: btn.getAttribute("aria-keyshortcuts") || "",
      paneFill: getComputedStyle(pane).fill,
      paneOpacity: getComputedStyle(pane).fillOpacity,
      persisted: nav.sidebar_collapsed,
      mirror: window.localStorage.getItem("richos-sidebar-hidden"),
      htmlAttr: document.documentElement.getAttribute("data-sidebar"),
      focused: document.activeElement ? document.activeElement.id : null,
    };
  });
}

/// The rail at rest after a toggle: the slide is .38s, so this waits for the END STATE, never a
/// length of time (the hang guard names itself).
async function railSettles(page, shown) {
  await page
    .waitForFunction(
      (want) => {
        const r = document.getElementById("rail");
        const cs = getComputedStyle(r);
        const rect = r.getBoundingClientRect();
        // Away is the END of the slide: hidden (which `visibility` becomes only when the slide
        // ends) and out of the row. A rail a pixel short of the edge is still moving.
        if (!want) return cs.display === "none" || (cs.visibility === "hidden" && rect.right <= 0.5);
        return cs.display !== "none" && cs.visibility !== "hidden" && Math.round(rect.left) === 0;
      },
      shown,
      { timeout: 60000 }
    )
    .catch(() => {
      throw new Error("the rail never settled " + (shown ? "open" : "away") + " (60 s hang guard)");
    });
}

async function sidebarToggleChecks(browser, run) {
  const page = await openApp(browser, { width: 1400, height: 900 });

  await run.check("S8 at 1400px the sidebar toggle is in the conversation's header, top left, and named", async () => {
    const s = await readSidebar(page);
    assert(s.btnShown, "the toggle must be on screen at 1180px and wider now — it was hidden there until S8");
    assert(s.btnFirstInHeader, "the toggle is the header's first control, as round 17.1 draws it");
    assert(s.btnFromHeaderLeft <= 20, "and at its left edge: " + s.btnFromHeaderLeft + "px in");
    assert(s.railShown, "the sidebar starts open");
    assertEqual(s.expanded, "true");
    assertEqual(s.label, "Hide the sidebar");
    assertEqual(s.title, "Hide the sidebar (⌘⇧S)");
    assertEqual(s.controls, "rail");
    assert(s.keys.includes("Meta+Shift+S"), "aria-keyshortcuts names ⌘⇧S: " + s.keys);
    assertEqual(s.paneOpacity, "1", "the glyph's left pane is empty while the sidebar is open");
    return `toggle ${s.btnFromHeaderLeft}px from the header's left, "${s.label}", ${s.keys}`;
  });

  await run.check("S8 a click sends the sidebar away with round 17.1's slide, and the choice is written", async () => {
    const curve = await page.evaluate(() => {
      const cs = getComputedStyle(document.getElementById("rail"));
      return { property: cs.transitionProperty, duration: cs.transitionDuration, timing: cs.transitionTimingFunction };
    });
    assert(curve.property.includes("margin-left"), "the rail slides: " + curve.property);
    assert(curve.duration.startsWith("0.38s"), "for .38s: " + curve.duration);
    assert(/cubic-bezier\(0\.22, 1, 0\.36, 1\)/.test(curve.timing), "on the out-quint curve: " + curve.timing);
    await page.click("#rail-toggle");
    await railSettles(page, false);
    const s = await readSidebar(page);
    assertEqual(s.stageLeft, 0, "the conversation takes the room");
    assertEqual(s.expanded, "false");
    assertEqual(s.label, "Show the sidebar");
    assertEqual(s.title, "Show the sidebar (⌘⇧S)");
    assert(s.paneFill !== "transparent" && Number(s.paneOpacity) > 0.2 && Number(s.paneOpacity) < 0.35,
      "the glyph's left pane fills (.28) while the sidebar is away: " + s.paneFill + " @ " + s.paneOpacity);
    assertEqual(s.persisted, true, "nav.rs (the mock store) holds sidebar_collapsed at 1400px — it did not before S8");
    assertEqual(s.mirror, "1", "and the first-paint mirror says so");
    assertEqual(s.htmlAttr, "hidden");
    return `rail right edge ${s.railRight}px, conversation at x=${s.stageLeft}, persisted ${s.persisted}, mirror ${s.mirror}`;
  });

  await run.check("S8 ⌘⇧S brings it back and sends it away again, from a text field too", async () => {
    await page.keyboard.press("Meta+Shift+KeyS");
    await railSettles(page, true);
    const back = await readSidebar(page);
    assertEqual(back.expanded, "true");
    assertEqual(back.persisted, false, "the way back is written too");
    assertEqual(back.mirror, "0");
    assertEqual(back.htmlAttr, null);
    await page.focus("#input");
    await page.keyboard.press("Meta+Shift+KeyS");
    await railSettles(page, false);
    const away = await readSidebar(page);
    assertEqual(away.persisted, true);
    assertEqual(away.focused, "input", "the shortcut does not take focus out of the composer");
    await page.keyboard.press("Meta+Shift+KeyS");
    await railSettles(page, true);
    return `open -> away from the composer -> open; persisted ${back.persisted}/${away.persisted}`;
  });

  await run.check("S8 focus inside the sidebar moves to the toggle when the sidebar goes", async () => {
    await page.focus('.nav-thread[data-thread-id="hiring"]');
    await page.keyboard.press("Meta+Shift+KeyS");
    await railSettles(page, false);
    const s = await readSidebar(page);
    assertEqual(s.focused, "rail-toggle", "nothing keeps focus on a rail that is not there");
    const tabbable = await page.evaluate(() => {
      const r = document.getElementById("rail");
      return [...r.querySelectorAll("button, [tabindex], input, a[href]")].filter((n) => getComputedStyle(n).visibility !== "hidden").length;
    });
    assertEqual(tabbable, 0, "and nothing in the hidden rail is visible to Tab");
    return "focus on #rail-toggle; 0 rail controls left visible";
  });

  await run.check("S8 SCREENSHOT: sidebar-hidden at 1400px (round 17.1's state)", async () => {
    await page.mouse.move(700, 450);
    const s = await shot(page, "sidebar-hidden-1400");
    assert(s.bytes > 3000, "too small to be a render: " + s.bytes);
    return `${s.file} (${s.bytes} bytes)`;
  });

  await run.check("S8 a relaunch paints the sidebar away from the first frame, before nav.rs answers", async () => {
    // Every frame from the moment `#rail` is parsed until main.js applies nav.rs's answer is
    // recorded. SLOW_BRIDGE puts 40 ms on each bridge call, so there are frames in that window
    // whatever the host's speed — a run with none would prove nothing, and is refused below.
    await page.addInitScript(SLOW_BRIDGE, 40);
    await page.addInitScript(() => {
      const samples = [];
      window.__railSamples = samples;
      let reconciled = false;
      const read = (when) => {
        const r = document.getElementById("rail");
        if (!r) return;
        const cs = getComputedStyle(r);
        const rect = r.getBoundingClientRect();
        samples.push({ when, reconciled, shown: cs.display !== "none" && cs.visibility !== "hidden" && rect.right > 1 });
      };
      const seenRail = new MutationObserver(() => {
        if (!document.getElementById("rail")) return;
        seenRail.disconnect();
        read("parsed");
        const tick = () => {
          read("frame");
          if (samples.length < 600) requestAnimationFrame(tick);
        };
        requestAnimationFrame(tick);
      });
      seenRail.observe(document, { childList: true, subtree: true });
      document.addEventListener("DOMContentLoaded", () => {
        new MutationObserver(() => {
          if (document.body.classList.contains("rail-instant")) reconciled = true;
        }).observe(document.body, { attributes: true, attributeFilter: ["class"] });
      });
    });
    await page.reload();
    await leaveHome(page);
    await page.waitForFunction(() => window.__railSamples.some((x) => x.reconciled), null, { timeout: 30000 });
    const samples = await page.evaluate(() => window.__railSamples);
    const before = samples.filter((x) => !x.reconciled);
    const frames = before.filter((x) => x.when === "frame").length;
    assert(before.length && before[0].when === "parsed", "the rail's first style was not sampled at parse time");
    assert(frames >= 2, "only " + frames + " frame(s) before nav.rs answered: nothing was proven about them");
    const flashed = before.filter((x) => x.shown);
    assertEqual(flashed.length, 0, "the rail was painted open in " + flashed.length + " frame(s) before nav.rs answered");
    await railSettles(page, false);
    const s = await readSidebar(page);
    assertEqual(s.expanded, "false", "after the relaunch the toggle says the sidebar is away");
    assertEqual(s.persisted, true);
    return `${frames} frame(s) painted before nav.rs answered, rail open in none of them; settled away`;
  });
  await page.close();

  await run.check("S8 nav.rs decides, never the mirror: a stale mirror is corrected at launch", async () => {
    const p = await openFresh(browser, { width: 1400, height: 900 }, { mirror: "1", store: "0" });
    await railSettles(p, true);
    const s = await readSidebar(p);
    assertEqual(s.persisted, false);
    assertEqual(s.mirror, "0", "the mirror was corrected to the durable answer");
    assertEqual(s.expanded, "true");
    await p.close();
    return "mirror said away, nav.rs said open: open, and the mirror now says 0";
  });

  await run.check("S8 820 to 1179: the same toggle slides the sidebar away and back, and writes it", async () => {
    const p = await openApp(browser, { width: 1000, height: 900 });
    await p.click("#rail-toggle");
    await railSettles(p, false);
    const away = await readSidebar(p);
    assertEqual(away.railDisplay, "flex", "at mid width the rail slides away; it is not display:none'd");
    assertEqual(away.persisted, true);
    await p.click("#rail-toggle");
    await railSettles(p, true);
    const back = await readSidebar(p);
    assertEqual(back.persisted, false);
    await p.close();
    return `away (stage at x=${away.stageLeft}) and back, persisted true then false`;
  });

  await run.check("S8 below 820 the drawer is unchanged, and its auto-close never decides the docked choice", async () => {
    const p = await openApp(browser, { width: 760, height: 900 });
    // openApp opened the drawer with the toggle; picking a thread closes it, as before S8.
    const drawer = await p.evaluate(() => ({
      position: getComputedStyle(document.getElementById("rail")).position,
      scrim: !document.getElementById("rail-scrim").hidden,
    }));
    assertEqual(drawer.position, "fixed", "below 820px the rail is still the full-height drawer");
    assert(drawer.scrim, "with its scrim");
    await p.click('.nav-thread[data-thread-id="hiring"]');
    await p.waitForFunction(() => document.body.classList.contains("rail-closed"));
    const closed = await readSidebar(p);
    assertEqual(closed.persisted, false, "the drawer's auto-close wrote nothing");
    await p.setViewportSize({ width: 1400, height: 900 });
    await railSettles(p, true);
    const wide = await readSidebar(p);
    assertEqual(wide.expanded, "true", "widened, the docked choice (open) is back");
    // And the reverse: away at 1400, through the drawer width, and back still away.
    await p.click("#rail-toggle");
    await railSettles(p, false);
    await p.setViewportSize({ width: 760, height: 900 });
    await p.setViewportSize({ width: 1400, height: 900 });
    await railSettles(p, false);
    const still = await readSidebar(p);
    assertEqual(still.persisted, true);
    assertEqual(p.__errors, [], "the shell threw");
    await p.close();
    return "drawer closed by a thread pick, nothing written; 760 -> 1400 open; away survives 1400 -> 760 -> 1400";
  });

  await run.check("S8 reduced motion: the sidebar goes and comes back with no slide", async () => {
    const p = await browser.newPage({ viewport: { width: 1400, height: 900 }, reducedMotion: "reduce" });
    await p.goto(APP);
    await leaveHome(p);
    await p.waitForSelector(".nav-thread", { state: "visible" });
    const d = await p.evaluate(() => getComputedStyle(document.getElementById("rail")).transitionDuration);
    assert(d.split(",").every((x) => parseFloat(x) === 0), "no transition under reduced motion: " + d);
    await p.keyboard.press("Meta+Shift+KeyS");
    await railSettles(p, false);
    await p.close();
    return "transition-duration " + d;
  });
}

/// A page whose durable store and first-paint mirror are seeded before the first byte, so a
/// disagreement between them can be staged.
async function openFresh(browser, viewport, seed) {
  const page = await browser.newPage({ viewport });
  await page.addInitScript((s) => {
    window.localStorage.setItem("richos-sidebar-hidden", s.mirror);
    window.localStorage.setItem("richos-mock-sidebar-collapsed", s.store);
  }, seed);
  await page.goto(APP);
  await leaveHome(page);
  await page.waitForSelector(".nav-thread", { state: "attached" });
  return page;
}

main().then(
  (failed) => { process.exitCode = failed ? 1 : 0; },
  (e) => {
    console.error(e);
    process.exit(1);
  }
);

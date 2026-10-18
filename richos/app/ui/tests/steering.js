// §25 "Steering and stop", criterion by criterion — the renderer against REAL BYTES, and
// the shell against the real composer.
//
// WHY REAL BYTES AND NOT A HAND-WRITTEN FIXTURE. `screencapture` on this machine has
// returned an all-black PNG for four slices running (the display is locked), so a photo of
// the app proves nothing. The precedent slice 5 set instead is a two-legged proof:
//
//   leg 1  `cargo run --example stop_payload` (app/src-tauri) runs a REAL turn behind an
//          `Arc<Mutex<Spine>>` exactly as the Tauri shell holds it, presses stop from
//          another thread through the same `TurnControl::request_stop` the `stop_turn`
//          command calls, and prints what `get_timeline` puts on the wire.
//   leg 2  THIS FILE renders those exact bytes with the real renderer under WebKit.
//
// `STOPPED_ON_THE_WIRE` below is the verbatim output of leg 1, captured 2026-08-29. Nothing
// in it was typed by hand. Where a test needs a case the run did not produce (a crash, a
// steering message), the change is made HERE, in one place, and named — so what is real and
// what is derived never has to be guessed at.
//
// Run: node steering.js   (or `npm test` for every suite in this directory)

"use strict";

const path = require("path");
const { leaveHome, loadPlaywright, openFixture, shot, createRun, assert, assertEqual, UI_DIR } = require("./lib/harness");

const APP = "file://" + path.join(UI_DIR, "index.html");

// ---------------------------------------------------------------------------------------
// THE REAL BYTES
//
// Verbatim stdout of `cargo run --example stop_payload -- --json`, 2026-08-29. The turn ran
// for a measured 1027ms (`endedAt - startedAt` = 1787995712883 - 1787995711856) and 35 of
// 120 scripted chunks reached the ledger before the stop landed, which is why the
// `rich_message` text below stops at "part 34".
// ---------------------------------------------------------------------------------------
const STOPPED_ON_THE_WIRE = {
  entityId: "harbor",
  items: [
    {
      bindingRevision: 1,
      createdAt: 1787995711820,
      entityId: "harbor",
      id: "turn_ffd7cf12576040cfa1b6cbd188db711f:user",
      kind: "user_message",
      sequence: null,
      slot: "opening",
      source: "text",
      text: "draft the whole memory strategy",
      threadId: "thr_152dcf24fbdc40d9827b2fd3ae277423",
      turnId: "turn_ffd7cf12576040cfa1b6cbd188db711f",
      visibility: "ceo",
    },
    {
      bindingRevision: 1,
      createdAt: 1787995711856,
      entityId: "harbor",
      id: "turn_ffd7cf12576040cfa1b6cbd188db711f:text:0",
      kind: "rich_message",
      phase: "unknown",
      sequence: 0,
      slot: "stream",
      text:
        "part 0. part 1. part 2. part 3. part 4. part 5. part 6. part 7. part 8. part 9. " +
        "part 10. part 11. part 12. part 13. part 14. part 15. part 16. part 17. part 18. " +
        "part 19. part 20. part 21. part 22. part 23. part 24. part 25. part 26. part 27. " +
        "part 28. part 29. part 30. part 31. part 32. part 33. part 34. ",
      threadId: "thr_152dcf24fbdc40d9827b2fd3ae277423",
      turnId: "turn_ffd7cf12576040cfa1b6cbd188db711f",
      visibility: "ceo",
    },
    {
      activeMs: 1027,
      bindingRevision: 1,
      createdAt: 1787995711820,
      endedAt: 1787995712883,
      entityId: "harbor",
      id: "turn_ffd7cf12576040cfa1b6cbd188db711f:duration",
      kind: "work_duration",
      sequence: null,
      slot: "terminal",
      startedAt: 1787995711856,
      state: "stopped",
      threadId: "thr_152dcf24fbdc40d9827b2fd3ae277423",
      turnId: "turn_ffd7cf12576040cfa1b6cbd188db711f",
      visibility: "ceo",
    },
  ],
  mode: "ceo",
  threadId: "thr_152dcf24fbdc40d9827b2fd3ae277423",
};

const clone = (v) => JSON.parse(JSON.stringify(v));

/// THE NEGATIVE CONTROL. The same bytes with ONE field changed — `state: "stopped"` becomes
/// `state: "interrupted"` — which is exactly what the ledger writes for a crash or a
/// rotation. If the renderer said "You stopped after 1s" here it would be blaming the CEO
/// for a compute-lease failure, which is the precise defect slice 5 refused to ship.
function interruptedOnTheWire() {
  const snap = clone(STOPPED_ON_THE_WIRE);
  snap.items.find((i) => i.kind === "work_duration").state = "interrupted";
  return snap;
}

/// A SECOND turn whose CEO message was created INSIDE the first turn's measured span —
/// which is what a steering message is. Nothing on the wire flags it; the cue is derived
/// from these timestamps (see `turnsOf` in timeline.js), so this fixture is built the only
/// way a real one ever gets built.
///
/// The first turn's span here is [1787995711856, 1787995712883]. The steering message is
/// stamped 1787995712400 — inside it. The first turn's OWN message is stamped
/// 1787995711820, before its own start, so it must not be cued.
function steeredOnTheWire() {
  const snap = clone(STOPPED_ON_THE_WIRE);
  const t2 = "turn_steering000000000000000000002";
  snap.items.push({
    bindingRevision: 1,
    createdAt: 1787995712400,
    entityId: "harbor",
    id: t2 + ":user",
    kind: "user_message",
    sequence: null,
    slot: "opening",
    source: "text",
    text: "also check the invoice while you're in there",
    threadId: snap.threadId,
    turnId: t2,
    visibility: "ceo",
  });
  // STOPPED WITH THE TURN IT WAS STEERING, and never handed to a lease — which is exactly
  // what `Spine::settle_stop_claim` does to steering written before the stop request. So
  // this fixture also covers §6.1's other stopped variant: a turn with no span to report.
  snap.items.push({
    activeMs: null,
    bindingRevision: 1,
    createdAt: 1787995712400,
    endedAt: 1787995712883,
    entityId: "harbor",
    id: t2 + ":duration",
    kind: "work_duration",
    sequence: null,
    slot: "terminal",
    startedAt: null,
    state: "stopped",
    threadId: snap.threadId,
    turnId: t2,
    visibility: "ceo",
  });
  return snap;
}

// ---------------------------------------------------------------------------------------
// The shell, with the three slice-6 commands intercepted.
//
// `mock.js` rejects unknown commands, which is correct — it is another engineer's file and
// this suite does not touch it. So the bridge is wrapped from OUTSIDE, before page scripts
// run, by intercepting the assignment to `window.RichBridge`. `main.js` holds that same
// object, so wrapping its two methods is enough and nothing is stubbed that already works.
// ---------------------------------------------------------------------------------------
async function openApp(browser, viewport, opts) {
  const page = await browser.newPage({ viewport: viewport || { width: 1400, height: 900 } });
  if (opts && opts.reducedMotion) await page.emulateMedia({ reducedMotion: "reduce" });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push("console: " + m.text());
  });
  await page.addInitScript(() => {
    window.__calls = [];
    window.__stub = {};
    window.__listeners = {};
    let real = null;
    Object.defineProperty(window, "RichBridge", {
      configurable: true,
      get() {
        return real;
      },
      set(v) {
        const origInvoke = v.invoke.bind(v);
        const origListen = v.listen.bind(v);
        v.invoke = async (cmd, args) => {
          window.__calls.push({ cmd, args: args || {} });
          if (Object.prototype.hasOwnProperty.call(window.__stub, cmd)) {
            const stubbed = window.__stub[cmd];
            if (stubbed === "__REJECT__") throw "stubbed rejection: " + cmd;
            return stubbed;
          }
          return origInvoke(cmd, args);
        };
        v.listen = async (name, cb) => {
          (window.__listeners[name] = window.__listeners[name] || []).push(cb);
          return origListen(name, cb);
        };
        real = v;
      },
    });
    // Drive the additive §13 family straight into main.js's own handlers — the same entry
    // point the Tauri emitter uses, so nothing about the UI's path is bypassed.
    window.__emit = (name, payload) => {
      for (const cb of window.__listeners[name] || []) cb({ payload });
    };
  });
  await page.goto(APP);
  // The home screen is the landing surface now; this suite is about the app UI behind it.
  await leaveHome(page);
  await page.waitForFunction("typeof window.RichTimeline === 'object'");
  await page.waitForSelector(".nav-thread", { state: "attached" });
  const railHidden = await page.evaluate(() => {
    const r = document.getElementById("rail");
    return getComputedStyle(r).display === "none" || document.body.classList.contains("rail-closed");
  });
  if (railHidden) await page.click("#rail-toggle");
  await page.waitForSelector(".nav-thread", { state: "visible" });
  // BY ID, never "the first one". `mock.js` is another engineer's file and its seed grows:
  // a thread added with a newer `last_activity` sorts above whatever used to be first, and
  // a suite that clicks position 1 starts opening an empty thread and times out with no
  // hint why. `acme` is the seeded thread with real history.
  await page.click('.nav-thread[data-thread-id="acme"]');
  await page.waitForSelector(".tl-turn");
  page.__errors = errors;
  return page;
}

/// Put the open thread into §11 `working` through the authoritative event, never by poking
/// the model. Returns the turn id it invented.
async function startTurn(page) {
  return page.evaluate(() => {
    const model = window.__RICHOS_TIMELINE__();
    const turnId = "turn_live_1";
    const started = Date.now() - 4000;
    const base = {
      entityId: model.entityId,
      threadId: model.threadId,
      bindingRevision: 1,
      turnId,
      at: Date.now(),
    };
    window.__emit("rich://turn-status", Object.assign({}, base, { status: "queued", startedAt: null }));
    window.__emit("rich://turn-status", Object.assign({}, base, { status: "working", startedAt: started }));
    return turnId;
  });
}

const composerState = (page) =>
  page.evaluate(() => {
    const stop = document.getElementById("stop");
    const send = document.getElementById("send");
    const input = document.getElementById("input");
    return {
      stopHidden: stop.hidden,
      stopDisabled: stop.disabled,
      sendHidden: send.hidden,
      inputDisabled: input.disabled,
      placeholder: input.placeholder,
      mode: document.getElementById("composer-row").dataset.mode,
      stopLabel: stop.getAttribute("aria-label"),
    };
  });

const durationLabels = (page) =>
  page.evaluate(() => Array.from(document.querySelectorAll(".tl-duration-label")).map((n) => n.textContent));

// ---------------------------------------------------------------------------------------

async function main() {
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();
  const run = createRun("§25 Steering and stop — real bytes + real shell, WebKit");

  for (const newThread of [true, false]) {
    await run.check(`sending from ${newThread ? "New thread" : "the company overview"} does not steer a pending conversation`, async () => {
      const p = await openApp(browser);
      try {
        await startTurn(p);
        await p.evaluate(() => {
          window.__previousModel = window.__RICHOS_TIMELINE__();
        });
        const group = p.locator("section.nav-group").filter({has: p.locator('.nav-thread[data-thread-id="acme"]')});
        await group.locator(newThread ? ".nav-group-add" : ".nav-group-label").click();
        await p.evaluate(() => { window.__calls = []; });
        await p.fill("#input", "An independent conversation");
        assert(await p.locator("#send").isVisible(), "Send stayed hidden after leaving the pending conversation");
        if (newThread) await p.click("#send");
        else await p.press("#input", "Enter");
        await p.waitForFunction("window.__calls.some(c => c.cmd === 'create_thread_in' || c.cmd === 'steer_message')");
        const calls = await p.evaluate(() => window.__calls.map(c => c.cmd));
        assert(!calls.includes("steer_message"), "new conversation was routed as steering to the pending one");
        assert(calls.includes("create_thread_in"), "no new conversation was created");
        await p.waitForFunction("window.__calls.some(c => c.cmd === 'send_message')");
        const state = await p.evaluate(() => ({
          original: window.__previousModel.threadId,
          current: window.__RICHOS_TIMELINE__().threadId,
          pending: window.__previousModel.turns.get("turn_live_1").live,
          answers: window.__calls.filter(c => c.cmd === "stop_turn"),
          sent: window.__calls.find(c => c.cmd === "send_message").args.text,
        }));
        assert(state.current !== state.original, "send kept the previous conversation binding");
        assertEqual(state.sent, "An independent conversation");
        assertEqual(state.pending, true, "opening a new conversation ended the pending turn");
        assertEqual(state.answers, [], "navigation stopped work");
        assertEqual(p.__errors, [], "renderer errors");
        return "independent send with the original turn still pending";
      } finally { await p.close(); }
    });
  }

  // ===================================================================================
  // §25 criterion 4: "The terminal row reads `You stopped after {duration}`."
  // ===================================================================================
  const fixture = await openFixture(browser);

  await run.check("§25 the terminal row reads `You stopped after 1s`, from the real wire bytes", async () => {
    await fixture.evaluate((snap) => window.__render(snap), STOPPED_ON_THE_WIRE);
    const r = await fixture.evaluate(() => ({
      labels: Array.from(document.querySelectorAll(".tl-duration-label")).map((n) => n.textContent),
      tone: document.querySelector(".tl-duration").dataset.tone,
      aria: document.querySelector(".tl-duration-btn").getAttribute("aria-label"),
    }));
    // 1027ms rendered under §6.2's rules: 1s to 59s renders as whole seconds, so 1027ms is
    // `1s`. Not 1.027s, not 1027ms, not "a moment".
    assertEqual(r.labels, ["You stopped after 1s"], "the §6.1 label");
    return `tone=${r.tone}  aria="${r.aria}"`;
  });

  await run.check("the stopped row is NOT painted as a failure", async () => {
    const r = await fixture.evaluate(() => {
      const row = document.querySelector(".tl-duration");
      const label = row.querySelector(".tl-duration-label");
      return {
        tone: row.dataset.tone,
        colour: getComputedStyle(label).color,
        failureCards: document.querySelectorAll(".tl-intervention").length,
      };
    });
    // §5.5's failure treatment belongs to failures. The CEO pressing his own button is not
    // one, and `--danger` (#8a4944 => rgb(138, 73, 68)) would tell him something broke.
    assertEqual(r.tone, "ceo-stopped", "its own tone, not the crash tone");
    assert(r.colour !== "rgb(138, 73, 68)", `the label is painted --danger: ${r.colour}`);
    assertEqual(r.failureCards, 0, "no failure card under a stop");
    assertEqual(await fixture.getByRole("button", { name: "Restore my message", exact: true }).count(), 1,
      "a saved stopped snapshot with partial prose must remain restorable");
    return `colour=${r.colour}  failure cards=${r.failureCards}`;
  });

  await run.check("NEGATIVE CONTROL: a crash is never attributed to the CEO", async () => {
    await fixture.evaluate((snap) => window.__render(snap), interruptedOnTheWire());
    const r = await fixture.evaluate(() => ({
      labels: Array.from(document.querySelectorAll(".tl-duration-label")).map((n) => n.textContent),
      tone: document.querySelector(".tl-duration").dataset.tone,
    }));
    assertEqual(r.labels, ["Stopped after 1s"], "the crash label is unchanged");
    assert(!r.labels[0].includes("You"), "a crash must never say `You`");
    assertEqual(r.tone, "stopped", "the crash keeps the crash tone");
    return `"${r.labels[0]}" tone=${r.tone} — same bytes, one field different`;
  });

  // ===================================================================================
  // §25 criterion 3: "Stop preserves all partial output and side-effect evidence."
  // ===================================================================================
  await run.check("§25 the partial answer is on screen after the stop", async () => {
    await fixture.evaluate((snap) => window.__render(snap), STOPPED_ON_THE_WIRE);
    const r = await fixture.evaluate(() => {
      const prose = Array.from(document.querySelectorAll(".tl-rich")).map((n) => n.textContent);
      return { count: prose.length, first: prose[0] || "", ceo: document.querySelector(".tl-user-text").textContent };
    });
    assert(r.count === 1, `expected one prose run, got ${r.count}`);
    assert(r.first.includes("part 0.") && r.first.includes("part 34."), "the partial run is truncated or missing");
    assert(!r.first.includes("part 35."), "more text rendered than the ledger holds");
    assertEqual(r.ceo, "draft the whole memory strategy", "the CEO's own message survives");
    return `35 of 120 scripted chunks rendered, ending at "part 34."`;
  });

  // ===================================================================================
  // §25 criterion 2: "A steering message joins the active turn in durable order."
  // ===================================================================================
  await run.check("§25 the steering cue is DERIVED from durable timestamps, not a session flag", async () => {
    await fixture.evaluate((snap) => window.__render(snap), steeredOnTheWire());
    const r = await fixture.evaluate(() => ({
      cues: Array.from(document.querySelectorAll(".tl-steer-cue")).map((n) => n.textContent),
      cuedText: Array.from(document.querySelectorAll(".tl-user.is-steering .tl-user-text")).map((n) => n.textContent),
      order: Array.from(document.querySelectorAll(".tl-user-text")).map((n) => n.textContent),
      // Always visible: the cue must not be hover-revealed furniture.
      opacity: getComputedStyle(document.querySelector(".tl-steer-cue")).opacity,
    }));
    assertEqual(r.cues, ["Added while Rich was working"], "exactly one cue");
    assertEqual(r.cuedText, ["also check the invoice while you're in there"], "the cue is on the steering message");
    assertEqual(
      r.order,
      ["draft the whole memory strategy", "also check the invoice while you're in there"],
      "durable order"
    );
    assertEqual(r.opacity, "1", "the cue must be visible without hovering");
    // §6.1's OTHER stopped variant: a turn stopped before it was ever handed to a lease has
    // no span, so the row says only what is known.
    const labels = await fixture.evaluate(() =>
      Array.from(document.querySelectorAll(".tl-duration-label")).map((n) => n.textContent)
    );
    assertEqual(labels, ["You stopped after 1s", "You stopped it"], "both §6.1 stopped variants");
    return `cue on the second message only; the first is not cued because its own turn's span excludes it. Rows: ${labels.join(" / ")}`;
  });

  const stoppedShot = await shot(fixture, "steering-stopped-row");

  // ===================================================================================
  // §18 accessibility, at renderer level
  // ===================================================================================
  await run.check("§18 the stopped row's accessible name carries the attribution and the meaning", async () => {
    await fixture.evaluate((snap) => window.__render(snap), STOPPED_ON_THE_WIRE);
    const aria = await fixture.evaluate(() => document.querySelector(".tl-duration-btn").getAttribute("aria-label"));
    assert(aria.startsWith("You stopped after 1s"), `accessible name: ${aria}`);
    assert(aria.includes("Active time"), "the name explains what the number means");
    return `"${aria}"`;
  });

  await run.check("§18 status is never colour alone — the word `You` carries the attribution", async () => {
    const r = await fixture.evaluate(() => {
      const label = document.querySelector(".tl-duration-label");
      return { text: label.textContent, colour: getComputedStyle(label).color };
    });
    assert(r.text.includes("You"), "the attribution must be in the words");
    return `"${r.text}" (${r.colour})`;
  });

  await fixture.close();

  // ===================================================================================
  // THE SHELL: §9.1 vs §9.2, and the stop control itself
  // ===================================================================================
  const page = await openApp(browser);

  await run.check("§9.1 idle: no stop control exists when there is nothing to stop", async () => {
    const s = await composerState(page);
    assertEqual(s.stopHidden, true, "stop is hidden while idle");
    assertEqual(s.sendHidden, false, "send is visible while idle");
    assertEqual(s.placeholder, "Talk to Rich…", "the §9.1 placeholder");
    assertEqual(s.mode, "idle");
    return "a stop button with nothing to stop teaches the CEO the control is decorative";
  });

  await run.check("§25 the composer remains usable during work", async () => {
    await startTurn(page);
    const s = await composerState(page);
    assertEqual(s.inputDisabled, false, "§9.2: the composer remains ENABLED");
    assertEqual(s.placeholder, "Add context or steer Rich…", "the §9.2 placeholder");
    assertEqual(s.stopHidden, false, "the stop control arms when work starts");
    assertEqual(s.mode, "working");
    return `placeholder="${s.placeholder}"  stop armed`;
  });

  await run.check("§9.2 stop REPLACES send on an empty composer, and SITS BESIDE it once he types", async () => {
    const empty = await composerState(page);
    assertEqual(empty.sendHidden, true, "empty + working: stop stands in for send");
    await page.fill("#input", "one more thing");
    const typed = await composerState(page);
    assertEqual(typed.sendHidden, false, "with words in the box, send comes back");
    assertEqual(typed.stopHidden, false, "and stop stays");
    return "he never has to choose between sending what he typed and stopping";
  });

  await run.check("§9.2 typing does not stop work", async () => {
    const stops = await page.evaluate(() => window.__calls.filter((c) => c.cmd === "stop_turn").length);
    assertEqual(stops, 0, "typing invoked stop_turn");
    await page.fill("#input", "");
    return "no stop_turn from any keystroke";
  });

  await run.check("§9.2 Enter during work submits the original input through the ordinary send path", async () => {
    await page.evaluate(() => {
      window.__calls = [];
      window.__stub.send_message = {};
    });
    await page.fill("#input", "also check the invoice");
    await page.press("#input", "Enter");
    await page.waitForFunction("window.__calls.some(c => c.cmd === 'send_message')");
    // The bubble is added to the model synchronously and painted on the next frame
    // (`scheduleRender`), so this waits for the paint rather than racing it.
    await page.waitForFunction(
      "Array.from(document.querySelectorAll('.tl-user-text')).some(n => n.textContent === 'also check the invoice')"
    );
    const r = await page.evaluate(() => ({
      cmds: window.__calls.map((c) => c.cmd),
      text: (window.__calls.find((c) => c.cmd === "send_message") || {}).args.text,
      bubbles: Array.from(document.querySelectorAll(".tl-user-text")).map((n) => n.textContent),
      composer: document.getElementById("input").value,
    }));
    assert(r.cmds.includes("send_message"), `expected send_message, saw ${r.cmds}`);
    assert(!r.cmds.includes("steer_message"), "a follow-up must not use a separate input lane");
    assertEqual(r.text, "also check the invoice", "the words reach the intake verbatim");
    assert(r.bubbles.includes("also check the invoice"), "§9.2: it renders as a CEO bubble immediately");
    assertEqual(r.composer, "", "the composer clears once the words are accepted");
    return `send_message("${r.text}") through the same input lane`; 
  });

  await run.check("§9.2 words the intake refuses are given back, never swallowed", async () => {
    await page.evaluate(() => {
      window.__calls = [];
      window.__stub.send_message = "__REJECT__";
    });
    await page.fill("#input", "this one gets refused");
    await page.press("#input", "Enter");
    await page.waitForFunction("document.getElementById('input').value === 'this one gets refused'");
    // `.tl-notice` as well as `.tl-rich`: this sentence is the app reporting on itself, and
    // since 2026-09-17 a local notice renders as a status line rather than as something Rich
    // said (`timeline.js`'s `renderLocalNotice`, Ray's candidate-.4 finding #8). The words
    // asserted below are unchanged — only the lane they land in moved.
    await page.waitForFunction(
      "Array.from(document.querySelectorAll('.tl-rich, .tl-notice')).some(n => n.textContent.includes('back in the box'))"
    );
    const notice = await page.evaluate(() =>
      Array.from(document.querySelectorAll(".tl-rich, .tl-notice")).map((n) => n.textContent).join(" | ")
    );
    assert(notice.includes("back in the box"), `expected a Rich-voiced explanation, got: ${notice}`);
    await page.evaluate(() => {
      delete window.__stub.send_message;
    });
    await page.fill("#input", "");
    return "the sentence is returned to the composer with a calm explanation, no stack trace";
  });

  // ===================================================================================
  // §9.3 stop, through the shell
  // ===================================================================================
  await run.check("§9.3 clicking stop calls stop_turn and the row moves to `Stopping`", async () => {
    await page.evaluate(() => {
      window.__calls = [];
      window.__stub.stop_turn = { stopped: true, turnId: "turn_live_1", requestedAt: Date.now(), reachedLease: true };
    });
    await page.click("#stop");
    await page.waitForFunction(
      "Array.from(document.querySelectorAll('.tl-duration-label')).some(n => n.textContent.startsWith('Stopping'))"
    );
    const r = await page.evaluate(() => ({
      cmds: window.__calls.map((c) => c.cmd),
      stop: window.__calls.find((c) => c.cmd === "stop_turn"),
      thread: window.__RICHOS_TIMELINE__().threadId,
      labels: Array.from(document.querySelectorAll(".tl-duration-label")).map((n) => n.textContent),
      mode: document.getElementById("composer-row").dataset.mode,
      inputDisabled: document.getElementById("input").disabled,
    }));
    assert(r.cmds.includes("stop_turn"), `expected stop_turn, saw ${r.cmds}`);
    assertEqual(r.stop.args.expectedThreadId, r.thread, "Stop identifies the displayed conversation");
    assert(!Object.hasOwn(r.stop.args, "expectedTurnId"), "the UI must not guess the backend's folded-input owner");
    // §11: `stopping` keeps the timer RUNNING, so the label carries a duration. It belongs
    // to the LIVE turn, which is the last row in a thread that already has history.
    const stopping = r.labels.filter((l) => /^Stopping/.test(l));
    assert(stopping.length === 1, `expected exactly one Stopping row, got ${JSON.stringify(r.labels)}`);
    assertEqual(r.mode, "stopping", "§11: the controls go quiet");
    assertEqual(r.inputDisabled, false, "the TEXT FIELD stays live — losing a half-typed sentence is not a feature");
    return `"${stopping[0]}" — controls disabled, keyboard not taken away`;
  });

  await run.check("§9.3 the authoritative `stopped` event replaces it with the §6.1 row", async () => {
    await page.evaluate(() => {
      const model = window.__RICHOS_TIMELINE__();
      const t = model.turns.get("turn_live_1");
      window.__emit("rich://turn-status", {
        entityId: model.entityId,
        threadId: model.threadId,
        bindingRevision: 1,
        turnId: "turn_live_1",
        status: "stopped",
        startedAt: t.startedAt,
        activeDurationMs: 18000,
        at: Date.now(),
      });
    });
    await page.waitForFunction(
      "Array.from(document.querySelectorAll('.tl-duration-label')).some(n => n.textContent.startsWith('You stopped'))"
    );
    const labels = await durationLabels(page);
    const s = await composerState(page);
    // 18000ms under §6.2: `18s`, which is the doc's own worked example.
    assert(labels.includes("You stopped after 18s"), `expected the §6.1 row, got ${JSON.stringify(labels)}`);
    assertEqual(s.stopHidden, true, "§9.3 step 6: normal send behavior is restored");
    assertEqual(s.sendHidden, false, "send is back");
    assertEqual(s.placeholder, "Talk to Rich…", "the §9.1 placeholder is restored");
    assertEqual(s.mode, "idle");
    return `"You stopped after 18s" — composer back to idle`;
  });

  await run.check("§9.3 a stop that reached nothing SAYS it reached nothing", async () => {
    const turn2 = await page.evaluate(() => {
      const model = window.__RICHOS_TIMELINE__();
      window.__emit("rich://turn-status", {
        entityId: model.entityId,
        threadId: model.threadId,
        bindingRevision: 1,
        turnId: "turn_live_2",
        status: "working",
        startedAt: Date.now() - 2000,
        at: Date.now(),
      });
      window.__calls = [];
      window.__stub.stop_turn = {
        stopped: true,
        turnId: "turn_live_2",
        requestedAt: Date.now(),
        reachedLease: false,
      };
      return "turn_live_2";
    });
    await page.click("#stop");
    // The same move as the refusal above: a stop that reached nothing is the app reporting on
    // itself, and it renders as a status line now.
    await page.waitForFunction(
      "Array.from(document.querySelectorAll('.tl-rich, .tl-notice')).some(n => n.textContent.includes('may finish on its own'))"
    );
    const notice = await page.evaluate(
      () => Array.from(document.querySelectorAll(".tl-rich, .tl-notice")).map((n) => n.textContent).filter((t) => t.includes("stopped this"))[0]
    );
    assert(notice && notice.includes("nothing new will start"), `expected the honest notice, got: ${notice}`);
    return `${turn2}: "${notice.trim().slice(0, 96)}…"`;
  });

  await run.check("§9.3 stopping nothing says nothing", async () => {
    await page.evaluate(() => {
      const model = window.__RICHOS_TIMELINE__();
      window.__emit("rich://turn-status", {
        entityId: model.entityId,
        threadId: model.threadId,
        bindingRevision: 1,
        turnId: "turn_live_2",
        status: "stopped",
        startedAt: Date.now() - 2000,
        activeDurationMs: 2000,
        at: Date.now(),
      });
      window.__calls = [];
      window.__stub.stop_turn = { stopped: false, turnId: null, requestedAt: null, reachedLease: false };
    });
    const before = await page.evaluate(() => document.querySelectorAll(".tl-rich").length);
    // The control is hidden now, so this is the programmatic equivalent of a stray keypress.
    await page.evaluate(() => document.getElementById("stop").click());
    const after = await page.evaluate(() => document.querySelectorAll(".tl-rich").length);
    assertEqual(after, before, "a no-op stop must not add a row");
    return "no announcement, no row, no change — the honest answer to stopping nothing";
  });

  // ===================================================================================
  // §18 accessibility, through the shell
  // ===================================================================================
  await run.check("§18 the stop control works by keyboard and names itself", async () => {
    await startTurn(page);
    const r = await page.evaluate(async () => {
      const stop = document.getElementById("stop");
      window.__calls = [];
      window.__stub.stop_turn = { stopped: true, turnId: "turn_live_1", requestedAt: Date.now(), reachedLease: true };
      stop.focus();
      const focused = document.activeElement === stop;
      // A real <button>, so Enter and Space activate it natively — asserted through the
      // element's own semantics rather than by synthesizing a click.
      return { focused, tag: stop.tagName, type: stop.type, label: stop.getAttribute("aria-label") };
    });
    assertEqual(r.tag, "BUTTON", "a real button, not a div with a click handler");
    assertEqual(r.type, "button", "not a submit — it must not fire on Enter in the text field");
    assert(r.focused, "the stop control is not focusable");
    assertEqual(r.label, "Stop Rich", "the accessible name");
    await page.press("#stop", "Enter");
    await page.waitForFunction("window.__calls.some(c => c.cmd === 'stop_turn')");
    return `<button type="button" aria-label="Stop Rich"> — Enter activates it`;
  });

  await run.check("§18 the stop glyph is decorative and never read aloud", async () => {
    const hidden = await page.evaluate(() => document.querySelector(".stop-glyph").getAttribute("aria-hidden"));
    assertEqual(hidden, "true", "the square must be aria-hidden; the label carries the meaning");
    return "the square is a picture of a stop, not the name of one";
  });

  await run.check("§18 reduced motion: the live/stopping mark is present and does not move", async () => {
    // Through the REAL event path on a REAL armed turn, not by poking the model — a check
    // that has to fabricate its own state cannot tell "no animation" from "no element".
    const rm = await openApp(browser, { width: 1400, height: 900 }, { reducedMotion: true });
    await startTurn(rm);
    await rm.evaluate(() => {
      window.__stub.stop_turn = { stopped: true, turnId: "turn_live_1", requestedAt: Date.now(), reachedLease: true };
    });
    await rm.click("#stop");
    await rm.waitForFunction(
      "Array.from(document.querySelectorAll('.tl-duration-label')).some(n => n.textContent.startsWith('Stopping'))"
    );
    const r = await rm.evaluate(() => {
      const p = document.querySelector(".tl-pulse");
      if (!p) return { present: false };
      const cs = getComputedStyle(p);
      return { present: true, animation: cs.animationName, opacity: cs.opacity };
    });
    // §17.4/§25: "Reduced-motion mode has no pulse animations." The MARK stays — it is the
    // status — it simply stops moving.
    assert(r.present, "the live mark vanished entirely under reduced motion; the status went with it");
    assertEqual(r.animation, "none", "the pulse is still animating under prefers-reduced-motion");
    await rm.close();
    return `mark present, animation-name=${r.animation}, opacity=${r.opacity}`;
  });

  // ===================================================================================
  // §20 responsive — the stop control at all three breakpoints
  // ===================================================================================
  for (const [w, band] of [
    [1400, "1180px and wider"],
    [1000, "820px to 1179px"],
    [760, "below 820px"],
  ]) {
    await run.check(`§20 the stop control is reachable and unclipped at ${w}px (${band})`, async () => {
      const p = await openApp(browser, { width: w, height: 900 });
      await startTurn(p);
      const r = await p.evaluate(() => {
        const stop = document.getElementById("stop");
        const input = document.getElementById("input");
        const sr = stop.getBoundingClientRect();
        const ir = input.getBoundingClientRect();
        return {
          hidden: stop.hidden,
          w: Math.round(sr.width),
          h: Math.round(sr.height),
          insideViewport: sr.left >= 0 && sr.right <= window.innerWidth + 1,
          overlapsInput: sr.left < ir.right - 1,
          composerWidth: Math.round(document.getElementById("composer").getBoundingClientRect().width),
        };
      });
      assertEqual(r.hidden, false, "the stop control vanished at this width");
      assert(r.w >= 36 && r.h >= 36, `hit target is ${r.w}x${r.h}, below a comfortable 40px square`);
      assert(r.insideViewport, "the stop control is clipped by the viewport");
      assert(!r.overlapsInput, "the stop control overlaps the text field");
      const s = await shot(p, `steering-composer-${w}`);
      await p.close();
      // `distinct` is present once the harness's own flat-fill check lands beside this
      // suite; reported when it is there rather than asserted, so this file runs against
      // either version of `lib/harness.js`.
      const painted = s.distinct != null ? `, ${s.distinct} distinct colours` : "";
      return `${r.w}x${r.h} inside a ${r.composerWidth}px composer — ${path.basename(s.file)} (${s.bytes}B${painted})`;
    });
  }

  await run.check("Stop follows the backend owner before and after native follow-up admission", async () => {
    for (const owner of ["turn_live_1", "folded_followup"]) {
      const folded = await openApp(browser);
      try {
        await startTurn(folded);
        await folded.evaluate((turnId) => {
          const model = window.__RICHOS_TIMELINE__();
          const base = { entityId: model.entityId, threadId: model.threadId, bindingRevision: 1,
            turnId: "folded_followup", at: Date.now() };
          window.__emit("rich://turn-status", { ...base, status: "queued", startedAt: null });
          window.__emit("rich://turn-status", { ...base, status: "working", startedAt: Date.now() });
          window.__calls = [];
          window.__stub.stop_turn = { stopped: true, turnId, requestedAt: Date.now(), reachedLease: true };
        }, owner);
        await folded.click("#stop");
        const result = await folded.evaluate(() => {
          const model = window.__RICHOS_TIMELINE__();
          return { call: window.__calls.find((c) => c.cmd === "stop_turn"), thread: model.threadId,
            first: model.turns.get("turn_live_1").status, followup: model.turns.get("folded_followup").status };
        });
        assertEqual(result.call.args.expectedThreadId, result.thread, "Stop retains the displayed conversation");
        assert(!Object.hasOwn(result.call.args, "expectedTurnId"), "UI rows must not guess native admission");
        assertEqual(result.first, owner === "turn_live_1" ? "stopping" : "working", "first input status");
        assertEqual(result.followup, owner === "folded_followup" ? "stopping" : "working", "follow-up status");
        assertEqual(folded.__errors, [], "folded page errors");
      } finally { await folded.close(); }
    }
    return "one conversation witness; only the backend-reported owner is marked Stopping";
  });

  await run.check("no page errors anywhere in the shell run", async () => {
    assertEqual(page.__errors, [], "console/page errors");
    return "clean";
  });

  await page.close();
  await browser.close();

  const painted = stoppedShot.distinct != null ? `, ${stoppedShot.distinct} distinct colours` : "";
  console.log(`\n  evidence: ${stoppedShot.file} (${stoppedShot.bytes}B${painted})`);
  const failed = run.report();
  process.exit(failed ? 1 : 0);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

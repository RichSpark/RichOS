// **CODE THAT IS ONLY CORRECT WHEN THE BRIDGE IS INSTANT** — the class of defect, with a lever
// that makes it visible on demand.
//
// Two shipped defects were measured on 2026-09-06 by `tom-opus-bw1` while replacing this
// directory's blind waits with real signals, and raised as `esc-20260906T085912Z-4005a6b5`.
// Both were the same shape: an interaction whose correctness depended on a bridge call
// returning before something else happened, which is true in an in-page mock and false on a
// Tauri IPC call. Neither is a test artifact and neither could be seen at zero latency.
//
//   1. A THREAD PRESSED DURING BOOT COULD OPEN A DIFFERENT THREAD. `main.js`'s `init()` painted
//      real, handler-wired `.nav-thread` buttons and then made five more bridge calls before
//      opening the thread the LAST session ended on — over the top of the one the CEO had just
//      pressed, with no error. At 120 ms, pressing "hiring" landed him on "general": Harbor
//      Analytics, "Running", zero turns.
//
//   2. THE UPDATE CONTROL COULD NOT BE REACHED IN ONE KEYSTROKE. `updates.js`'s `openTheRow()`
//      deferred the cue's focus with `setTimeout(..., 0)`, which fired while `#update-install`
//      was still `disabled` by the menu's own in-flight `update_state` read. Focus landed at
//      0 ms of bridge latency and never at 10 ms or more, so §26's "one keystroke from the act"
//      held in a zero-latency fixture and nowhere else.
//
// THE LEVER IS THE POINT OF THIS FILE. `INSTRUMENTED_BRIDGE` puts a fixed latency on every call
// the page makes, wrapped at the ASSIGNMENT of `window.RichBridge` so every caller in the page is
// behind it. A check that is green at 0 ms and green at 400 ms is a check that waited for a
// signal; one that is green at 0 and red at 120 was measuring the machine. Each check below runs
// at THREE latencies — 0, 120 (an ordinary machine, and the number both defects were measured at)
// and 400 (slower still) — and states which, so a green line here is a statement about all three.
//
// WHAT THESE CHECKS ARE NOT. They are not a shorter window. Both fixes removed a DEPENDENCE on
// timing — a restore that yields to a choice, a ticket that abandons a stale chain, a focus bound
// to the control becoming usable — so the assertions below are about outcomes at any latency
// rather than about a race being won more often. If a future change reintroduces a window, these
// go red at 120 and 400 while staying green at 0, which is the signature to look for.
//
// PROVEN TO CATCH THE DEFECTS IT NAMES, not merely observed green. `app/ui/main.js` and
// `app/ui/updates.js` were restored to their pre-fix state at `041eee8`, this file left exactly
// as it stands, and the run reported 6 of its 9 checks red — verbatim, trimmed to the first line
// of each:
//
//     FAIL  a thread pressed during boot is the thread that opens (120ms bridge)
//             pressed "hiring" at +360ms and the conversation on screen is "Harbor Analytics /
//             Running" — the rail marks "general", the model is bound to "general", 0 turn(s)
//     FAIL  a thread pressed during boot is the thread that opens (400ms bridge)
//             pressed "hiring" at +1200ms ... the model is bound to "general", 0 turn(s) painted
//     FAIL  the shell remembers the thread he pressed, not the one the boot would have imposed
//             at 400ms ... the shell would restore "general"
//     FAIL  the update cue puts the hand on the control (0ms bridge)
//     FAIL  the update cue puts the hand on the control (120ms bridge)
//     FAIL  the update cue puts the hand on the control (400ms bridge)
//             the menu's own re-read must not disarm the control it opens onto
//     FAIL  the row's control is never disarmed by the menu's own re-read (400ms bridge)
//             the control was not pressable on 25 of 55 sampled frames
//
// THE THREE THAT STAYED GREEN ARE THE CONTROL CASES, and they are why the list above is evidence
// rather than a suite that fails at everything: the 0 ms thread press was correct before the fix
// (the defect needs latency), and "with no press at all, the boot still restores where the last
// session ended" was correct before it and must stay correct after — a fix that read "never
// restore" would have broken exactly that line.
//
// The cue check goes red at 0 ms as well, and that is not a stricter reading of the old defect —
// it is the same one seen without the fixture's help. The old `setTimeout(0)` fired after the
// microtask that resolved a 0 ms invoke, which is the entire reason the defect was invisible
// here; reading the focus in the SAME synchronous block as the press removes that accident.
//
// Run: node slow-bridge.js   (or `npm test` for every suite in this directory)

"use strict";

const path = require("path");
const { loadPlaywright, leaveHome, createRun, assert, assertEqual, UI_DIR } = require("./lib/harness");

const APP = "file://" + path.join(UI_DIR, "index.html");

/// The latencies every check is run at. 120 is where both defects were measured; 400 is the
/// "slower still" the fix has to survive as well, and it is not a fantasy — it is a laptop
/// swapping, or a spine mid-rotation.
const LATENCIES = [0, 120, 400];

/// A DELIBERATE SLOW RUNNER, WITH A LEDGER: `ms` of latency on every bridge call the page makes,
/// and a record of every call issued and returned.
///
/// Wrapped at the ASSIGNMENT of `window.RichBridge` rather than after load, so it is in front of
/// every caller including the ones `init()` makes before any test code runs. Passed to
/// `page.addInitScript` before `goto`.
///
/// THE LEDGER IS HOW THIS FILE KNOWS THE BOOT IS OVER, and it is here rather than in the product
/// because a test's end state must not need the product to publish one. See `bootDone`.
///
/// `tom-opus-bw1` wrote the latency half first — inline in `contrast.js`, then hoisted into
/// `lib/harness.js` on his branch as `SLOW_BRIDGE`. It is spelled out here rather than imported
/// because that branch is not landed; when it lands, the latency half should become
/// `require("./lib/harness").SLOW_BRIDGE` and the two must not be allowed to drift.
const INSTRUMENTED_BRIDGE = (ms) => {
  window.__bridgeIssued = [];
  window.__bridgeReturned = [];
  // The words of every `send_message`, in the order the page issued them (check 9).
  window.__sentTexts = [];
  let real;
  Object.defineProperty(window, "RichBridge", {
    configurable: true,
    get: () => real,
    set: (v) => {
      real = v;
      const invoke = v.invoke.bind(v);
      v.invoke = async (name, args) => {
        window.__bridgeIssued.push(name);
        if (name === "send_message") window.__sentTexts.push(args && args.text);
        if (ms > 0) await new Promise((r) => setTimeout(r, ms));
        try {
          return await invoke(name, args);
        } finally {
          window.__bridgeReturned.push(name);
        }
      };
    },
  });
};

/// A NEGATIVE OBSERVATION WINDOW — not a wait for a signal, and named differently on purpose so
/// nobody later "fixes" it into one. Two of the assertions here are that over a stretch of time
/// NOTHING happens: no second thread opens, no control is disarmed. There is no end state to wait
/// for, because the claim is about the absence of one. The number IS the assertion's width.
async function observe(page, ms, why) {
  void why;
  await page.waitForTimeout(ms);
}

async function open(browser, lag) {
  const page = await browser.newPage({ viewport: { width: 1200, height: 800 } });
  const errors = [];
  page.on("pageerror", (e) => errors.push("pageerror: " + e.message));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push("console: " + m.text());
  });
  await page.addInitScript(INSTRUMENTED_BRIDGE, lag);
  await page.goto(APP);
  await leaveHome(page);
  page.__errors = errors;
  return page;
}

/// `init()` HAS RUN TO ITS LAST LINE — not "the app looks ready", which is a different and much
/// earlier fact.
///
/// The end state is `raw_retention`, which `syncRetentionFromBackend()` issues from the very last
/// line of `init()`. Once it has RETURNED, every decision `init()` makes about which screen the
/// CEO is on has been made, and any override it was going to issue has been issued.
///
/// A COMPOSER HOLDING FOCUS IS NOT THIS FACT, and believing it was made the first version of this
/// suite fail against a correct product. `home.js:1037` focuses `#input` when the home screen is
/// hidden — "Focus follows the surface" — so `leaveHome()` satisfies that condition before
/// `init()` has reached its rail, let alone its landing branch. Measured 2026-09-06: at 120 ms
/// the check read `bound: null` on a boot that went on to land perfectly well.
async function bootDone(page) {
  await page.waitForFunction(() => (window.__bridgeReturned || []).indexOf("raw_retention") >= 0, null, {
    timeout: 60000,
  });
}

async function onScreen(page) {
  return page.evaluate(() => {
    const m = window.__RICHOS_TIMELINE__ ? window.__RICHOS_TIMELINE__() : null;
    return {
      active: (document.querySelector(".nav-thread-row.is-active .nav-thread") || { dataset: {} }).dataset.threadId,
      bound: m ? m.threadId : null,
      entity: ((document.getElementById("scope-entity") || {}).textContent || "").trim(),
      crumb: ((document.getElementById("scope-thread") || {}).textContent || "").trim(),
      turns: document.querySelectorAll("#messages .tl-turn").length,
    };
  });
}

/// Press a thread row `after` ms after it first exists, and report where the app ends up.
///
/// The offsets the callers use are not decoration. `init()` reaches the rail on its 7th bridge
/// call and its landing branch 5 calls later, so a press was overwritten when its own two-call
/// chain (`switch_thread`, `active_context`) could not land before that branch — between 3 and 5
/// call-times after the row appears. Sampling at 3, 3.5 and 4 call-times covers that window with
/// room for the jitter a real machine adds, rather than pinning one number measured on one Mac.
async function pressDuringBoot(browser, lag, after) {
  const page = await open(browser, lag);
  await page.waitForSelector('.nav-thread[data-thread-id="hiring"]', { state: "attached" });
  if (after > 0) await page.waitForTimeout(after);
  await page.evaluate(() => document.querySelector('.nav-thread[data-thread-id="hiring"]').click());
  await bootDone(page);
  await observe(
    page,
    6 * lag + 400,
    "the override this check is about was issued before `raw_retention` and needs about four " +
      "more call-times to paint, so the window is six of them plus a floor for a 0ms run"
  );
  const on = await onScreen(page);
  const remembered = await page.evaluate(() => window.RichBridge.invoke("active_thread"));
  const errors = page.__errors.slice();
  await page.close();
  return { on, remembered, errors };
}

/// Seed the panel with an update waiting, exactly as `updates.rs` reports one.
const AVAILABLE = {
  state: "available",
  currentVersion: "0.1.1",
  availableVersion: "0.1.2",
  busy: false,
  busyReason: null,
  notes: null,
  pubDate: null,
  downloadedBytes: 0,
  totalBytes: null,
  percent: null,
  endpoint: "",
  endpointIsPlaceholder: true,
  checkedAt: null,
  failure: null,
};

async function withUpdateWaiting(browser, lag) {
  const page = await open(browser, lag);
  await page.waitForSelector(".nav-thread", { state: "attached" });
  await bootDone(page);
  await page.evaluate((v) => window.__RICHOS_MOCK__.updateSet(Object.assign({}, v, { checkedAt: Date.now() }), []), AVAILABLE);
  await page.waitForSelector("#update-cue", { state: "visible" });
  return page;
}

async function main() {
  const run = createRun("slow-bridge — the interactions that were correct only at zero latency");
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();

  // ---- 1. a thread pressed during boot is the thread that opens -------------------------
  for (const lag of LATENCIES) {
    await run.check(`a thread pressed during boot is the thread that opens (${lag}ms bridge)`, async () => {
      // Three presses across the window `init()` used to own: 3, 3.5 and 4 call-times after the
      // row appears. At 0 ms they are all "immediately", which is the control case.
      const offsets = [3 * lag, 3.5 * lag, 4 * lag].map(Math.round);
      const seen = [];
      for (const after of offsets) {
        const r = await pressDuringBoot(browser, lag, after);
        assertEqual(r.errors, [], `the shell logged errors on the press at +${after}ms`);
        assertEqual(
          r.on.bound,
          "hiring",
          `pressed "hiring" at +${after}ms and the conversation on screen is ` +
            JSON.stringify(r.on.entity + " / " + r.on.crumb) +
            ` — the rail marks ${JSON.stringify(r.on.active)}, the model is bound to ` +
            `${JSON.stringify(r.on.bound)}, ${r.on.turns} turn(s) painted. A press that opens a ` +
            `different conversation than the one pressed is the defect, not a slow machine`
        );
        assertEqual(r.on.active, "hiring", `the rail marks the thread he pressed at +${after}ms`);
        assert(r.on.turns > 0, `and its turns are painted, not the previous thread's: ${r.on.turns}`);
        seen.push("+" + after + "ms");
      }
      return `pressed at ${seen.join(", ")} — every one landed on Northwind Traders / "Q4 hiring" with its own turn`;
    });
  }

  // ---- 2. and the shell REMEMBERS the thread he pressed ----------------------------------
  await run.check("the shell remembers the thread he pressed, not the one the boot would have imposed", async () => {
    // A race here does not end when the window is repainted: `switch_thread` is what the NEXT
    // launch restores from, so a boot that issued its own behind his left the wrong thread
    // durably active even on a launch where the screen happened to end up right.
    const out = [];
    for (const lag of LATENCIES.filter((l) => l > 0)) {
      const r = await pressDuringBoot(browser, lag, Math.round(3.5 * lag));
      assertEqual(
        r.remembered,
        "hiring",
        `at ${lag}ms the screen shows ${JSON.stringify(r.on.bound)} and the shell would restore ` +
          `${JSON.stringify(r.remembered)} — the next launch would land him somewhere he never chose`
      );
      out.push(`${lag}ms: active_thread = "hiring"`);
    }
    return out.join("; ");
  });

  // ---- 3. ...and the restore itself still happens when he presses nothing -----------------
  await run.check("with no press at all, the boot still restores where the last session ended", async () => {
    // The other half, and the one a fix like this breaks if it is written as "never restore".
    const out = [];
    for (const lag of LATENCIES) {
      const page = await open(browser, lag);
      await bootDone(page);
      const on = await onScreen(page);
      const remembered = await page.evaluate(() => window.RichBridge.invoke("active_thread"));
      assertEqual(on.bound, "general", `at ${lag}ms the boot did not restore the last session's thread`);
      assertEqual(remembered, "general", `at ${lag}ms the shell's own memory disagrees with the screen`);
      assertEqual(page.__errors, [], `the shell logged errors during a plain boot at ${lag}ms`);
      await page.close();
      out.push(`${lag}ms: general`);
    }
    return "restored at " + out.join(", ");
  });

  // ---- 4. the update cue puts the hand on the control -------------------------------------
  for (const lag of LATENCIES) {
    await run.check(`the update cue puts the hand on the control (${lag}ms bridge)`, async () => {
      const page = await withUpdateWaiting(browser, lag);
      // PRESSED AND READ IN ONE SYNCHRONOUS BLOCK — no task turn, no frame, no round trip. This
      // is the strongest form of §26's "one keystroke from the act": the hand is on the control
      // before control returns to the event loop, so there is no window for anything to be
      // waited out. A deferred focus cannot pass this line at any latency, including 0.
      const immediate = await page.evaluate(() => {
        document.getElementById("update-cue").click();
        const go = document.getElementById("update-install");
        return {
          focus: document.activeElement && document.activeElement.id,
          hidden: !!(go && go.hidden),
          disabled: !!(go && go.disabled),
        };
      });
      assertEqual(immediate.disabled, false, "the menu's own re-read must not disarm the control it opens onto");
      assertEqual(
        immediate.focus,
        "update-install",
        "the hand is on the row's own control in the same turn as the press, not one task queue later"
      );
      // AND THE KEYSTROKE REACHES THE COMMAND. Focus on a control that does nothing would be a
      // more elaborate version of the same defect.
      await page.keyboard.press("Enter");
      await page.waitForFunction(() => window.__RICHOS_MOCK__.updateCalls().indexOf("update_install") >= 0, null, {
        timeout: 15000,
      });
      const calls = await page.evaluate(() => window.__RICHOS_MOCK__.updateCalls());
      assertEqual(page.__errors, [], "the shell logged errors between the cue and the command");
      await page.close();
      return `focus landed synchronously on #update-install and Enter issued update_install (${calls.join(", ")})`;
    });
  }

  // ---- 5. the control is never disarmed by the menu's own re-read -------------------------
  await run.check("the row's control is never disarmed by the menu's own re-read (400ms bridge)", async () => {
    // `update_state` is a pure READ (`updates.rs:767` — `refresh_work_verdict` then `snapshot`).
    // Sampling ACROSS the whole round trip is the assertion: at 400 ms there is a third of a
    // second in which the old code offered a dimmed, unpressable button to a CEO who had just
    // been told an update was waiting.
    const page = await withUpdateWaiting(browser, 400);
    await page.evaluate(() => {
      window.__disarmed = [];
      const sample = () => {
        const go = document.getElementById("update-install");
        window.__disarmed.push(!go ? "absent" : go.hidden ? "hidden" : go.disabled ? "disabled" : "live");
        if (window.__sampling) requestAnimationFrame(sample);
      };
      window.__sampling = true;
      document.getElementById("update-cue").click();
      sample();
    });
    await observe(page, 900, "one 400ms round trip and change — the whole length of the re-read the press sets off");
    const seen = await page.evaluate(() => {
      window.__sampling = false;
      return window.__disarmed;
    });
    const bad = seen.filter((s) => s !== "live");
    assert(seen.length > 10, `the sampler ran: ${seen.length} frame(s)`);
    assertEqual(bad, [], `the control was not pressable on ${bad.length} of ${seen.length} sampled frames`);
    assertEqual(page.__errors, [], "the shell logged errors while the row was re-read");
    await page.close();
    return `${seen.length} frames sampled across a 400ms read, live on every one`;
  });

  // ---- 3. a send made while a thread is still opening is not thrown away -----------------
  //
  // THE THIRD DEFECT OF THIS EXACT SHAPE, and it is the one a person actually met. Ray filed
  // it twice — audit-4 #18 and audit-7 row 10 — as "the first Return after clicking into the
  // composer does not send; the second does". His check 0 cost 26 seconds and two Returns on
  // the CEO's own screen.
  //
  // `send()` opened with `if (mainView === "opening") return;`. That view is the window
  // between a thread being asked for and its timeline arriving — six bridge round trips — and
  // through all of it the composer is editable and carries the IDLE placeholder `Talk to
  // Rich…`. At 0 ms of latency the window is a few milliseconds wide and the defect is
  // invisible, which is precisely why it belongs in this file: it is a correctness bug that
  // only exists when the bridge is not instant.
  for (const lag of LATENCIES.filter((l) => l > 0)) {
    await run.check(`a send issued while the thread is still opening is honored, not dropped (${lag}ms bridge)`, async () => {
      const page = await open(browser, lag);
      await page.waitForSelector('.nav-thread[data-thread-id="hiring"]', { state: "attached" });
      await bootDone(page);
      // Start from a settled desk on a DIFFERENT thread, so opening "hiring" is a real
      // navigation with a real opening window rather than the boot's own.
      await page.evaluate(() => document.querySelector('.nav-thread[data-thread-id="hiring"]').click());
      await page.waitForFunction(() => document.getElementById("composer-row").dataset.mode !== "opening", null, {
        timeout: 60000,
      });
      const other = await page.evaluate(() => {
        const rows = Array.from(document.querySelectorAll(".nav-thread")).map((n) => n.dataset.threadId);
        return rows.find((id) => id && id !== "hiring") || null;
      });
      assert(other, "the rail offers only one thread, so there is no navigation to type during");

      await page.evaluate((id) => document.querySelector(`.nav-thread[data-thread-id="${id}"]`).click(), other);
      // IN the window, and asserted to be in it rather than assumed: a check that typed after
      // the view had settled would pass without ever touching the defect.
      const during = await page.evaluate(() => ({
        mode: document.getElementById("composer-row").dataset.mode,
        placeholder: document.getElementById("input").placeholder,
        editable: !document.getElementById("input").disabled,
      }));
      assertEqual(during.mode, "opening", "the composer was not in the opening window, so nothing was tested");
      assert(during.editable, "the composer was disabled during the window — then a person could not type into it");

      const SENTENCE = "Check 0: reply with exactly: candidate seven walk begins.";
      await page.evaluate((t) => {
        const i = document.getElementById("input");
        i.focus();
        i.value = t;
        i.dispatchEvent(new Event("input", { bubbles: true }));
      }, SENTENCE);
      await page.keyboard.press("Enter");
      // A person who sees nothing happen presses it again. AT MOST ONE MESSAGE MAY RESULT.
      await page.keyboard.press("Enter");
      await page.keyboard.press("Enter");

      await page.waitForFunction(() => document.getElementById("composer-row").dataset.mode !== "opening", null, {
        timeout: 60000,
      });
      // BOUNDED, AND THE FAILURE IS NAMED RATHER THAN A TIMEOUT. With the replay removed this
      // wait can never be satisfied, and a 60s `waitForFunction` would report "Timeout
      // exceeded" — true, and useless to whoever has to read it. Six call-times is the whole
      // send path (`send_message` plus the events that paint its bubble) with room to spare,
      // and then the state is asserted with the sentence in the message. Verified by removing
      // `replayHeldSend(threadId)` from `openThread`: this check then goes red at both
      // latencies saying his sentence is still sitting in the composer.
      const arrived = await page
        .waitForFunction(
          (t) => Array.from(document.querySelectorAll(".tl-user-text")).some((n) => n.textContent.trim() === t),
          SENTENCE,
          { timeout: 6 * lag + 4000 }
        )
        .then(() => true)
        .catch(() => false);
      if (!arrived) {
        const stuck = await page.evaluate(() => ({
          box: document.getElementById("input").value,
          mode: document.getElementById("composer-row").dataset.mode,
          users: document.querySelectorAll(".tl-user-text").length,
        }));
        throw new Error(
          `the send made during the opening window never arrived: the composer still holds ` +
            `${JSON.stringify(stuck.box)}, the mode is ${JSON.stringify(stuck.mode)} and the thread shows ` +
            `${stuck.users} user message(s). This is audit-7 row 10 — his sentence was thrown away, and ` +
            `the second Return is what a person has to work out for himself`
        );
      }
      const after = await page.evaluate((t) => {
        const mine = Array.from(document.querySelectorAll(".tl-user-text")).filter(
          (n) => n.textContent.trim() === t
        );
        return {
          copies: mine.length,
          box: document.getElementById("input").value,
          thread: window.__RICHOS_TIMELINE__ ? window.__RICHOS_TIMELINE__().threadId : null,
        };
      }, SENTENCE);
      assertEqual(
        after.copies,
        1,
        `three Returns during the opening window produced ${after.copies} copies of his sentence — ` +
          `one dropped send is a defect and three duplicates is a worse one`
      );
      assertEqual(after.box, "", "the composer still holds a sentence that was sent");
      assertEqual(after.thread, other, `the held send landed on ${JSON.stringify(after.thread)} rather than the thread he typed it for`);
      assertEqual(page.__errors, [], "the shell logged errors while the held send was replayed");

      // Send submits exactly the words present at the press. Subsequent edits are a
      // new draft, even if navigation is still loading.
      await page.evaluate(() => document.querySelector('.nav-thread[data-thread-id="hiring"]').click());
      const mode2 = await page.evaluate(() => document.getElementById("composer-row").dataset.mode);
      assertEqual(mode2, "opening", "the second navigation had no opening window, so this half is testing nothing");
      await page.evaluate(() => {
        const i = document.getElementById("input");
        i.focus();
        i.value = "half a thought";
        i.dispatchEvent(new Event("input", { bubbles: true }));
      });
      await page.keyboard.press("Enter");
      await page.evaluate(() => {
        const i = document.getElementById("input");
        i.value = "half a thought, changed";
        i.dispatchEvent(new Event("input", { bubbles: true }));
      });
      await page.waitForFunction(() => document.getElementById("composer-row").dataset.mode !== "opening", null, {
        timeout: 60000,
      });
      await observe(page, 3 * lag + 400, "a send replayed on arrival would have painted its bubble within three call-times");
      const edited = await page.evaluate(() => ({
        box: document.getElementById("input").value,
        sent: Array.from(document.querySelectorAll(".tl-user-text")).map((n) => n.textContent.trim()),
      }));
      assert(
        edited.sent.includes("half a thought") && !edited.sent.includes("half a thought, changed"),
        `Send changed the submitted words or sent the later draft: ${JSON.stringify(edited.sent.slice(-3))}`
      );
      assertEqual(edited.box, "half a thought, changed", "his edited sentence is no longer in the composer");
      await page.close();
      return (
        `in the opening window the composer reads "${during.placeholder}" and is editable; three Returns ` +
        `produced exactly 1 message, on the right thread, with the box emptied; an edited sentence is ` +
        `left in the box and never sent`
      );
    });
  }

  // ---- 6. the send that CREATES the thread never empties the screen ----------------------
  //
  // THE FOURTH DEFECT OF THIS EXACT SHAPE, and Ray measured this one frame by frame on the
  // real window (candidate .11, §2, frames 04 and 05,
  // `docs/verification/2026-09-19-nightly-1.2.0-nightly.20260918.6-mac-and-android-audit.md`):
  //
  //     +0.28 s  message posted — the send itself is instant
  //     +5.6 s   still fine
  //     +6.3 s → +10.2 s  THE THREAD IS GONE. Replaced by the empty-thread greeting
  //              "I'm Rich — your chief of staff…". His message and the card have vanished.
  //     +10.9 s  message and card are back
  //
  // His words for it: *"Four and a half seconds is long enough to press things."* It is the
  // data-flash failure in its plainest form, and it is worse than a flash: what takes his
  // sentence's place is the screen a person sees when nothing has ever happened here.
  //
  // IT BELONGS IN THIS FILE because it is the same class as the three above — code that is
  // only correct when the bridge is instant. `send()`'s new-thread branch puts a DISPOSABLE
  // bubble in a throwaway model and awaits `openThread`, which builds a fresh model whose
  // `loadTimeline` clears it; from that moment until `send()` resumes AFTER `openThread`
  // returns, the model on screen is empty and `flushRender` draws the greeting over it. At
  // 0 ms the window is a frame or two; the 4.6 s Ray measured is `renderFirstRunNotice` and
  // `drainWorkNotices` waiting on a spine that the pre-prime holds for a whole model turn.
  //
  // The sampler is the assertion. A check that read the screen once, afterwards, would pass
  // against the defect every time — the end state was never wrong.
  //
  // PROVEN TO CATCH IT, measured rather than assumed. `app/ui/main.js` was restored to
  // `05ab7a0c` with this file left exactly as it stands:
  //
  //     PASS  … (0ms bridge)     the greeting on 0 of 150 frames
  //     FAIL  … (120ms bridge)   the first-run greeting was on screen for 8 of 223 frames
  //     FAIL  … (400ms bridge)   the first-run greeting was on screen for 24 of 391 frames
  //
  // **0 ms is the control case and it is supposed to pass**, exactly as the thread-press
  // check's 0 ms case is: the window is the length of the awaits, so with no bridge latency
  // there is nothing to see. And the mock's 130 ms and 400 ms are the floor rather than the
  // measurement — Ray's 4.6 s is those same awaits behind a spine mutex the pre-prime holds
  // for a whole model turn, which no in-page fixture has.
  for (const lag of LATENCIES) {
    await run.check(`the thread never empties to the first-run greeting while his message is in flight (${lag}ms bridge)`, async () => {
      const page = await open(browser, lag);
      await page.waitForSelector(".nav-thread", { state: "attached" });
      await bootDone(page);

      // The flow a person takes: "+ New chat or company", pick the company, type, Return
      // (round 32: the company is picked in the button's own menu).
      await page.evaluate(() => document.getElementById("rail-new-thread").click());
      const picked = await page.evaluate(() => {
        const first = document.querySelector("#new-chat-menu .m-item[data-company]");
        if (!first) return false;
        first.click();
        return true;
      });
      await page.waitForSelector("#entity-view:not([hidden])", { timeout: 6 * lag + 8000 });

      const SENTENCE = "add a line to notes.txt in the QA fixture repository and land it";
      await page.evaluate((sentence) => {
        window.__frames = [];
        const messages = document.getElementById("messages");
        const sample = () => {
          const visible = !!(messages && messages.offsetParent !== null);
          const text = messages ? messages.textContent : "";
          window.__frames.push({
            greeting: visible && text.includes("chief of staff"),
            mine: Array.from(document.querySelectorAll(".tl-user-text")).filter(
              (n) => n.textContent.trim() === sentence
            ).length,
          });
          if (window.__sampling) requestAnimationFrame(sample);
        };
        window.__sampling = true;
        const i = document.getElementById("input");
        i.focus();
        i.value = sentence;
        i.dispatchEvent(new Event("input", { bubbles: true }));
        // PRESSED IN THE SAME BLOCK THE SAMPLER STARTS IN, so the first frame after Return is
        // recorded. Ray's blank opens 6 s in; a sampler that started late would be measuring
        // the recovery rather than the gap.
        i.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
        // **THE FIRST SAMPLE IS AN ANIMATION FRAME, NOT THIS TICK, AND THAT IS A CORRECTION
        // RATHER THAN A CONVENIENCE.** Reading the DOM synchronously here measured a state the
        // browser never presented: `send()` runs `showConversationView()` (which unhides
        // `#messages`) and `scheduleRender()` in this same tick, and the renderer's own
        // `requestAnimationFrame` lands before the next paint — so the synchronous read sees
        // the PREVIOUS thread's DOM under a container that has just been unhidden, for zero
        // painted frames. Measured: exactly 1 greeting frame at 0, 120 and 400 ms, always
        // index 0, always at t+0 ms with the composer still reading `idle`. Sampling from a
        // frame callback registered after the renderer's makes every reading below a statement
        // about a frame that was actually put on screen, which is the only thing Ray could see.
        requestAnimationFrame(sample);
      }, SENTENCE);

      // Long enough to cover the whole chain the old code blanked across: create_thread_in,
      // refreshNavigation, openThread (switch_thread + active_context + get_timeline), plus
      // renderFirstRunNotice and the send itself — ten bridge round trips and a floor for 0 ms.
      await observe(page, 10 * lag + 2500, "the whole create-thread-and-send chain");
      const out = await page.evaluate((sentence) => {
        window.__sampling = false;
        return {
          frames: window.__frames,
          greetingNow: document.getElementById("messages").textContent.includes("chief of staff"),
          copies: Array.from(document.querySelectorAll(".tl-user-text")).filter(
            (n) => n.textContent.trim() === sentence
          ).length,
        };
      }, SENTENCE);

      const frames = out.frames;
      assert(frames.length > 10, `the sampler ran: ${frames.length} frame(s)`);
      assertEqual(page.__errors, [], "the shell logged errors while the thread was created");

      // (a) THE GREETING IS NEVER DRAWN. This is the defect itself.
      const blank = frames.filter((f) => f.greeting).length;
      assertEqual(
        blank,
        0,
        `the first-run greeting was on screen for ${blank} of ${frames.length} sampled frames — ` +
          `his message was replaced by the screen a person sees when nothing has ever happened ` +
          `here (Ray's candidate-.11 §2.2: 4.6 s of it, and long enough to press things)`
      );

      // (b) AND HIS SENTENCE NEVER LEAVES ONCE IT HAS ARRIVED. The greeting is what he saw;
      // the loss of his own words is what made it frightening, and a fix that drew nothing at
      // all in that window would pass (a) while leaving him looking at an empty conversation.
      const appeared = frames.findIndex((f) => f.mine > 0);
      assert(appeared >= 0, "his sentence never appeared at all during the send");
      const vanished = frames.slice(appeared).filter((f) => f.mine === 0).length;
      assertEqual(
        vanished,
        0,
        `his sentence was on screen and then gone again for ${vanished} of ` +
          `${frames.length - appeared} frames after it first appeared`
      );

      // (c) ONE COPY, NEVER TWO. The fix carries the bubble across the model replacement, and
      // a carry that also re-added it would draw the CEO's one sentence twice (§25).
      assertEqual(out.copies, 1, `his sentence is on screen ${out.copies} times after the send`);
      assertEqual(out.greetingNow, false, "the first-run greeting is on screen after his send landed");
      const doubled = frames.filter((f) => f.mine > 1).length;
      assertEqual(doubled, 0, `his sentence was drawn twice on ${doubled} sampled frames`);

      await page.close();
      return (
        `${frames.length} frames sampled across the create-thread-and-send chain` +
        `${picked ? " (company picked from the New chat or company menu)" : " (no company in the menu)"}: ` +
        `the greeting on 0 of them, his sentence present on every frame from ${appeared} onward, one copy`
      );
    });
  }

  // ---- 7. words typed while the launch is still opening his thread stay in the box -------
  //
  // THE FIFTH DEFECT OF THIS EXACT SHAPE (esc-20261020T171021Z-8fd0fca7). After a relaunch the
  // composer is on screen and typable while `init()` is still awaiting the reads in front of
  // its restore (in the test guest, first-send-walk.py's frame at +44 s after the relaunch shows
  // his typed words over a conversation the restore had not yet started opening). The restore's
  // `openThread` then emptied the box: `stashThreadViewState` parks the words only against a
  // thread already on screen, and at launch there is none yet, so they were dropped, and the
  // destination's (empty) draft was put in their place. A person who typed his first sentence
  // in that window saw it vanish, or pressed Send on an empty box and nothing reached Rich.
  //
  // TYPED WITH REAL KEY EVENTS and sent with the Return key, the way a person does it. 0 ms is
  // left out for the same reason as check 3: the window is the length of the awaits.
  for (const lag of LATENCIES.filter((l) => l > 0)) {
    await run.check(`words typed while the launch is still opening his thread are kept and sent (${lag}ms bridge)`, async () => {
      const page = await open(browser, lag);
      await page.waitForSelector("#input", { state: "visible" });
      const SENTENCE = "Please ask my team to read the first line of CLAUDE.md";
      await page.click("#input");
      await page.keyboard.type(SENTENCE);
      // IN the window, and asserted to be in it: typing after the restore had begun would pass
      // without ever touching the defect.
      const during = await page.evaluate(() => ({
        restoring: (window.__bridgeIssued || []).indexOf("switch_thread") >= 0,
        box: document.getElementById("input").value,
      }));
      assertEqual(during.restoring, false, "the launch had already started opening the thread, so nothing was tested");
      assertEqual(during.box, SENTENCE, "the keystrokes did not reach the composer");

      await bootDone(page);
      await observe(page, 6 * lag + 400, "the restore's openThread paints within a few call-times of the boot's end");
      const settled = await page.evaluate(() => ({
        box: document.getElementById("input").value,
        thread: window.__RICHOS_TIMELINE__ ? window.__RICHOS_TIMELINE__().threadId : null,
      }));
      assertEqual(settled.thread, "general", "the launch did not restore the last session's thread");
      assertEqual(
        settled.box,
        SENTENCE,
        `the words he typed while the launch was opening his thread are gone from the composer ` +
          `(it reads ${JSON.stringify(settled.box)}) — the swallowed first send`
      );

      await page.focus("#input");
      await page.keyboard.press("Enter");
      const arrived = await page
        .waitForFunction(
          (t) => Array.from(document.querySelectorAll(".tl-user-text")).some((n) => n.textContent.trim() === t),
          SENTENCE,
          { timeout: 60000 } // a hang guard: a held or kept send is in the thread or it never will be
        )
        .then(() => true)
        .catch(() => false);
      assert(arrived, "Return on the kept words did not put his message in the conversation");
      const after = await page.evaluate((t) => ({
        copies: Array.from(document.querySelectorAll(".tl-user-text")).filter((n) => n.textContent.trim() === t).length,
        box: document.getElementById("input").value,
      }), SENTENCE);
      assertEqual(after.copies, 1, `his sentence is on screen ${after.copies} times`);
      assertEqual(after.box, "", "the composer still holds a sentence that was sent");
      assertEqual(page.__errors, [], "the shell logged errors while the launch restored his thread");
      await page.close();
      return `typed before the restore began; the words were still in the box on "general" and Return sent them once`;
    });
  }

  // ---- 8. ...and Return pressed in that window sends them, once, to the thread it opens ------
  //
  // WHAT THE TEST GUEST MEASURED WITH REAL KEYS (first-send-walk.py, bundle a7db7f70b): typed and
  // Return pressed while no conversation was open yet; the composer emptied, no message appeared
  // and no PromptReceived row was written. `send()` sent with no thread, `send_message` refused
  // it (`thread_id: String`), and the restore then emptied the box the refusal had put the words
  // back in. The mock used to accept a send with no thread, which is why no check here saw it.
  for (const lag of LATENCIES.filter((l) => l > 0)) {
    await run.check(`Return pressed while the launch is still opening his thread sends his words once (${lag}ms bridge)`, async () => {
      const page = await open(browser, lag);
      await page.waitForSelector("#input", { state: "visible" });
      const SENTENCE = "First send after a relaunch, sent with the Return key";
      await page.click("#input");
      await page.keyboard.type(SENTENCE);
      await page.keyboard.press("Enter");
      const during = await page.evaluate((t) => ({
        restoring: (window.__bridgeIssued || []).indexOf("switch_thread") >= 0,
        box: document.getElementById("input").value,
        shown: Array.from(document.querySelectorAll(".tl-user-text")).some((n) => n.textContent.trim() === t),
      }), SENTENCE);
      assertEqual(during.restoring, false, "the launch had already started opening the thread, so nothing was tested");

      await bootDone(page);
      const arrived = await page
        .waitForFunction(
          (t) => Array.from(document.querySelectorAll(".tl-user-text")).some((n) => n.textContent.trim() === t) &&
            window.__RICHOS_TIMELINE__ && window.__RICHOS_TIMELINE__().threadId === "general",
          SENTENCE,
          { timeout: 60000 } // a hang guard: a held or kept send is in the thread or it never will be
        )
        .then(() => true)
        .catch(() => false);
      await observe(page, 6 * lag + 400, "a second copy, or a refusal, would arrive within a few call-times");
      const after = await page.evaluate((t) => ({
        copies: Array.from(document.querySelectorAll(".tl-user-text")).filter((n) => n.textContent.trim() === t).length,
        box: document.getElementById("input").value,
        thread: window.__RICHOS_TIMELINE__ ? window.__RICHOS_TIMELINE__().threadId : null,
        refused: (document.getElementById("messages").textContent || "").includes("invalid args"),
      }), SENTENCE);
      assert(
        arrived,
        `Return in the launch window never put his message in "general": the composer reads ` +
          `${JSON.stringify(after.box)} and the thread shows ${after.copies} copies — the swallowed first send`
      );
      assertEqual(after.copies, 1, `his sentence is on screen ${after.copies} times`);
      assertEqual(after.box, "", "the composer still holds the sentence that was sent");
      assertEqual(after.refused, false, "a refusal of a send with no thread is on screen");
      assertEqual(page.__errors, [], "the shell logged errors while the held send went out");
      await page.close();
      return (
        `Return pressed before the restore began; his message was ${during.shown ? "on screen at once" : "not yet drawn"}, ` +
        `the box ${JSON.stringify(during.box)}, and it reached "general" once`
      );
    });
  }

  // ---- 9. ...and a second Send while that thread opens never overtakes the held one ------
  //
  // Second review rv-20261020T181204Z-11d18147-df79: the held send went out only when the
  // opening had finished, but `openThread` binds `activeThreadId` before it awaits anything, so a
  // Send pressed in the opening window went straight out FIRST. "Please send the proposal
  // today." then "Correction: do not send it until I approve." reached Rich reversed.
  for (const lag of LATENCIES.filter((l) => l > 0)) {
    await run.check(`a Send pressed while the launch opens his thread goes after the held one (${lag}ms bridge)`, async () => {
      const page = await open(browser, lag);
      await page.waitForSelector("#input", { state: "visible" });
      const FIRST = "Please send the proposal today.";
      const SECOND = "Correction: do not send it until I approve.";
      await page.click("#input");
      await page.keyboard.type(FIRST);
      await page.keyboard.press("Enter");
      const held = await page.evaluate(() => (window.__bridgeIssued || []).indexOf("switch_thread") < 0);
      assert(held, "the launch had already started opening the thread, so nothing was held");
      // The restore has started opening the thread: the opening window, where a Send goes out at once.
      await page.waitForFunction(() => document.getElementById("composer-row").dataset.mode === "opening", null, {
        timeout: 60000, // a hang guard: the restore opens the thread or the boot is broken
      });
      await page.keyboard.type(SECOND);
      const mode = await page.evaluate(() => document.getElementById("composer-row").dataset.mode);
      assertEqual(mode, "opening", "the thread finished opening before the second Send, so the overtaking window was not tested");
      await page.keyboard.press("Enter");
      await bootDone(page);
      await page.waitForFunction(() => window.__sentTexts.length >= 2, null, {
        timeout: 60000, // a hang guard: both sends are issued or one was lost
      });
      await observe(page, 6 * lag + 400, "a third, duplicate send would be issued within a few call-times");
      const sent = await page.evaluate(() => window.__sentTexts.slice());
      assertEqual(sent, [FIRST, SECOND], "his two messages reached Rich in a different order from the one he sent them in");
      assertEqual(page.__errors, [], "the shell logged errors while the two sends went out");
      await page.close();
      return `held "${FIRST}", then sent "${SECOND}" in the opening window: send_message issued in his order, once each`;
    });
  }

  await browser.close();
  process.exit(run.report() > 0 ? 1 : 0);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

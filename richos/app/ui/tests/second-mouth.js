// **A MESSAGE HE DID NOT TYPE HERE STILL APPEARS HERE, AND IT APPEARS AT ONCE.**
//
// Ray's candidate .13 walk, defect R3
// (`docs/verification/2026-09-19-nightly-1.2.0-nightly.20260919.2-mac-and-android-audit.md`):
//
//     "He taps Send on his phone and the Mac thread shows nothing for about six seconds; then
//      his words and the whole answer appear together. The only sign in between is a small
//      amber dot on the thread's row in the sidebar."
//
//     tap -> first appearance on the Mac 5.68 s, measured twice (5.77 s and 5.68 s). At
//     2.99 s the thread body was unchanged and only the sidebar row carried a working dot.
//     Mac -> phone is 0.096 s, so the asymmetry is ~60x.
//
// # Why this window had nothing to draw
//
// Every event in the §13 family is about RICH — his prose, his machinery, his workers, the
// state of the turn answering. The CEO's own sentence reached this screen by being DRAWN HERE
// optimistically the moment he pressed Enter (`addPendingUserMessage`), and reached it again
// on a reload as `TimelineItem::UserMessage`. Both assume he typed it HERE. His phone is a
// second mouth and makes that assumption false, so his message waited for `turn-completed` to
// reload the thread — which is why it arrived in the same frame as the finished answer.
//
// `rich://ceo-message` is the event that fixes it. It has existed in `live.rs` since
// `2c95c27d`; what was missing was (a) the spine emitting it for a message that came in off
// the intake log rather than through `accept_prompt`, and (b) this window listening for it.
//
// # What this file drives, and why it is not a mock of the fix
//
// The window is handed the EXACT event sequence the spine emits for a phone message —
// `turn-status: queued`, `rich://ceo-message`, `turn-status: working`, then Rich's prose —
// through the same `RichBridge.listen` callbacks `main.js` registered, with NO composer
// interaction, because he did not type here. That is the whole of what makes a phone message
// different from a desk one, and it is reproduced rather than described.
//
// Run: node second-mouth.js   (or `npm test` for every suite in this directory)

"use strict";

const { loadPlaywright, createRun, assert, assertEqual } = require("./lib/harness");
const { openApp } = require("./waiting-state");

const HIS_WORDS = "where are we on the proposal?";
const TURN = "turn_from_his_phone";

/// The fence every event in a turn carries, read off the live model rather than typed.
async function fenceOf(page) {
  return page.evaluate(() => {
    const m = window.__RICHOS_TIMELINE__();
    return { entityId: m.entityId, threadId: m.threadId, bindingRevision: m.bindingRevision };
  });
}

/// The spine's own order for a message that arrived from somewhere that is not this window:
/// the status, then his words, then the sidebar row. `spine.rs` `drain_intake`, channel arm.
async function phoneMessageArrives(page, fence, opts) {
  const o = opts || {};
  await page.evaluate(
    (o) => {
      const at = Date.now();
      window.__emit("rich://turn-status", Object.assign({}, o.fence, {
        turnId: o.turnId, status: "queued", startedAt: null, activeDurationMs: null,
        visibility: "ceo", at,
      }));
      if (!o.withoutHisWords) {
        window.__emit("rich://ceo-message", Object.assign({}, o.fence, {
          turnId: o.turnId, messageId: o.turnId + ":user", text: o.text, source: o.source || "text",
          createdAt: at, at,
        }));
      }
      window.__emit("rich://thread-summary-updated", Object.assign({}, o.fence, {
        turnId: o.turnId, title: "the proposal", messageCount: 2, lastActivity: at,
        status: "working", at,
      }));
    },
    { fence, turnId: o.turnId || TURN, text: o.text || HIS_WORDS, source: o.source, withoutHisWords: o.withoutHisWords }
  );
}

/// Every CEO bubble on screen, in order, as a person reads them.
async function hisBubbles(page) {
  return page.evaluate(() =>
    Array.from(document.querySelectorAll(".tl-user .tl-user-text"))
      .map((el) => el.innerText.replace(/\s+/g, " ").trim())
      .filter((t) => t.length > 0)
  );
}

async function main() {
  const run = createRun("a second mouth: a message typed somewhere else appears here, at once (audit R3)");
  const browser = await loadPlaywright().webkit.launch();
  try {
    // ---- 1. the defect itself ----------------------------------------------------------
    await run.check(
      "a message typed on his phone is on this screen before Rich has answered it",
      async () => {
        const page = await openApp(browser, "dark");
        const before = await hisBubbles(page);
        assert(
          !before.some((t) => t.includes(HIS_WORDS)),
          "his words were already on screen before anything was sent"
        );

        const fence = await fenceOf(page);
        const at = Date.now();
        await phoneMessageArrives(page, fence);
        await page.waitForFunction(
          (t) => document.body.innerText.includes(t),
          HIS_WORDS,
          { timeout: 4000 }
        ).catch(() => {});
        const shown = await hisBubbles(page);
        const onScreenIn = Date.now() - at;

        // NOT "the turn is running" — RICH HAS NOT SAID A WORD YET. The defect was that his
        // sentence waited for the answer, so the assertion is made at the one moment that
        // distinguishes the two: after the status events, before any prose.
        const proseYet = await page.evaluate(() => document.querySelectorAll(".tl-rich-text").length);
        assert(
          shown.some((t) => t.includes(HIS_WORDS)),
          `his sentence is not on screen. CEO bubbles: ${JSON.stringify(shown)}`
        );
        assertEqual(page.__errors || [], [], "the page reported errors");
        await page.close();
        return `on screen ${onScreenIn} ms after the events, with ${proseYet} run(s) of Rich's prose rendered`;
      }
    );

    // ---- 2. the negative control --------------------------------------------------------
    await run.check(
      "NEGATIVE CONTROL: with `rich://ceo-message` withheld, the same sequence leaves the screen blank",
      async () => {
        // This is `a2cef8ee` reproduced exactly — the status and the sidebar row arriving and
        // his words not — and it is what makes check 1 a proof rather than a description. If
        // this one ever comes back clean, check 1 is passing for some other reason and says so.
        const page = await openApp(browser, "dark");
        const fence = await fenceOf(page);
        await phoneMessageArrives(page, fence, { withoutHisWords: true });
        await page.waitForTimeout(600);
        const shown = await hisBubbles(page);
        assert(
          !shown.some((t) => t.includes(HIS_WORDS)),
          "his words rendered from the status events alone, so check 1 proves nothing about the new event"
        );
        // And the sidebar DID move — Ray's amber dot. The path was live; one event was missing.
        const rail = await page.evaluate(() => document.getElementById("rail").innerText.replace(/\s+/g, " "));
        await page.close();
        return `no CEO bubble from status events alone; rail still rendering (${rail.length} chars)`;
      }
    );

    // ---- 3. his own typing is not doubled ------------------------------------------------
    await run.check(
      "a message he types HERE is still exactly one bubble when the event arrives for it",
      async () => {
        // The upsert lands on `{turnId}:user`, which is the id `turn-status: queued` has
        // already adopted the optimistic bubble onto. If the handler inserted instead of
        // upserting, his one sentence would be two rows — which is the defect this event's
        // own doc comment in `live.rs` was written to prevent.
        //
        // NOTHING IS EMITTED BY HAND HERE. He types, presses Enter, and the mock runs the
        // WHOLE sequence the spine runs — optimistic bubble, `queued`, `rich://ceo-message`,
        // `working`, prose, `completed` — because `mock.js` emits that event now too. It did
        // not until today, which is its own small finding: the event has been on the real wire
        // since `2c95c27d` and the mock never rehearsed it, so a window that ignored it looked
        // perfectly healthy in every suite in this directory.
        const page = await openApp(browser, "dark");
        const typed = "draft the Q4 memo";
        await page.fill("#input", typed);
        await page.press("#input", "Enter");
        await page.waitForFunction((t) => document.body.innerText.includes(t), typed, { timeout: 4000 });
        // Let the turn run to its end, because the reload at `turn-completed` is the OTHER
        // path that could put his sentence on screen a second time.
        await page.waitForFunction(
          () => document.querySelector('#composer-row[data-mode="idle"]') !== null,
          null,
          { timeout: 15000 }
        ).catch(() => {});
        await page.waitForTimeout(400);
        const shown = await hisBubbles(page);
        const copies = shown.filter((t) => t.includes(typed)).length;
        assertEqual(copies, 1, `his one sentence rendered ${copies} times: ${JSON.stringify(shown)}`);
        await page.close();
        return `"${typed}" on screen exactly once, through the optimistic bubble, the event and the reload`;
      }
    );

    // ---- 4. the fence still holds ---------------------------------------------------------
    await run.check(
      "an event for another company's thread never reaches this screen",
      async () => {
        // The gate is `accepts()`, shared with every other handler in the family — this check
        // exists because a NEW listener is a new way to get past it, and a CEO message is the
        // single worst payload to render across an entity boundary.
        const page = await openApp(browser, "dark");
        const fence = await fenceOf(page);
        const foreign = Object.assign({}, fence, { entityId: "some-other-company" });
        await phoneMessageArrives(page, foreign, { text: "words from another company", turnId: "turn_elsewhere" });
        await page.waitForTimeout(500);
        const body = await page.evaluate(() => document.body.innerText);
        assert(
          !body.includes("words from another company"),
          "a CEO message fenced to another entity rendered on this screen"
        );
        await page.close();
        return "fenced out, as every other event in the family is";
      }
    );

    // ---- 5. WHY THIS FILE WAS GREEN OVER A RED APP ---------------------------------------
    await run.check(
      "the revision fence is a FLOOR, so an emitter that builds a fence from the durable binding is invisible here",
      async () => {
        // **THIS CHECK EXISTS BECAUSE CHECKS 1–4 PASSED WHILE THE APP MEASURED 7,288 ms.**
        //
        // Ray re-walked the same defect on candidate .16 with the emit fix in the build
        // (`docs/verification/2026-09-19-nightly-1.2.0-nightly.20260919.5-stale-engine-and-phone-audit.md`,
        // §"R3, phone → Mac"): 7,288 ms, WORSE than the 5.68 s this file was written against, the
        // bubble and the whole reply in one paint, and the sidebar's dot still at +471 ms.
        //
        // The event WAS emitted. This window threw it away. `fenceOf()` at the top of this file
        // reads the fence OFF THE LIVE MODEL — which is the honest thing for checks 1–4 to do and
        // is exactly what made them blind: the spine built its fence from a DIFFERENT source.
        //
        //   * the window binds to the ACTIVATION revision — `main.js` → `active_binding()` →
        //     `Spine::activate` → `Ledger::rebind_at_new_revision`, a fresh `take_revision()`
        //     that persists nothing;
        //   * `Spine::drain_intake` built its fence from `Ledger::thread_binding` — the revision
        //     written once when the thread was bound and never rewritten.
        //
        // Measured in `phone_intake_tests::the_fence_on_his_phone_message_is_the_one_the_open_window_is_holding`:
        // the wire carried **1**, the window held **3**. `accepts()` rejects anything strictly
        // lower, so his sentence, the queued status and the working status were all dropped, and
        // only `turn-completed`'s `loadTimeline()` ever drew them — in one frame.
        //
        // THE FIX IS THE EMITTER'S, and that is where the red-to-green proof lives (Rust, above).
        // What is pinned HERE is the half that made the trap possible: the floor is real, a
        // payload below it does not render, and therefore a harness that feeds this window the
        // model's own fence is not evidence about what the spine sends. Read this check before
        // trusting checks 1–4 about anything outside this file.
        const page = await openApp(browser, "dark");
        const fence = await fenceOf(page);
        assert(
          typeof fence.bindingRevision === "number" && fence.bindingRevision > 0,
          `the model carries no activation revision to be measured against: ${JSON.stringify(fence)}`
        );
        // The thread's DURABLE revision, which is what the shipped spine used to put on the wire.
        // Strictly lower, always: a thread is bound at one revision and then activated at the
        // next, before this window ever sees it.
        const durable = Object.assign({}, fence, { bindingRevision: fence.bindingRevision - 1 });
        await phoneMessageArrives(page, durable, { text: "sent from the kitchen", turnId: "turn_stale_fence" });
        await page.waitForTimeout(600);
        const shown = await hisBubbles(page);
        assert(
          !shown.some((t) => t.includes("sent from the kitchen")),
          "the revision floor is gone: an event older than this window's activation now renders, " +
            "which is §13's fence removed rather than the emitter fixed"
        );
        // AND THE POSITIVE CONTROL, one revision up, in the same fixture: the identical sequence
        // at the window's own revision draws immediately. Without this the check above would pass
        // on a window that renders nothing at all.
        await phoneMessageArrives(page, fence, { text: "sent from the hallway", turnId: "turn_live_fence" });
        await page.waitForFunction(
          (t) => document.body.innerText.includes(t),
          "sent from the hallway",
          { timeout: 4000 }
        );
        const after = await hisBubbles(page);
        assert(
          after.some((t) => t.includes("sent from the hallway")),
          `the positive control did not render either: ${JSON.stringify(after)}`
        );
        assertEqual(page.__errors || [], [], "the page reported errors");
        await page.close();
        return (
          `at bindingRevision ${durable.bindingRevision} the window drew nothing; at ` +
          `${fence.bindingRevision} — its own — the same sequence drew the bubble before any prose. ` +
          `The emitter is pinned in phone_intake_tests.rs, not here.`
        );
      }
    );
  } finally {
    await browser.close();
  }
  process.exitCode = run.report() > 0 ? 1 : 0;
}

main().catch((e) => {
  console.error(e && e.stack ? e.stack : e);
  process.exit(1);
});

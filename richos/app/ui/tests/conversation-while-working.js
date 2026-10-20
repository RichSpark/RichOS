// THE CONVERSATION WHILE RICH WORKS — product hunt 2026-10-15 (ray-opus-hunt57), finding 13,
// against the REAL renderer under WebKit.
//
//   13. Rows that just say "Worked" appeared above Rich's messages: under "Worked for 15s",
//       above every message Rich sent on his own ("Rich reached out"), and while he was still
//       working. Acceptance: none of these rows.
//
// The nameless ACTIVITY row ("Worked queued") is removed in Rust (`timeline::activity_item`
// makes it technical, pinned by `a_tool_call_that_classifies_to_nothing_is_not_a_worked_row_in_his_view`);
// this suite holds the half the renderer owns: the duration divider.
//
// RUN:  node conversation-while-working.js
//       RICHOS_PLAYWRIGHT=/path/to/node_modules/playwright node conversation-while-working.js

"use strict";

const { loadPlaywright, openFixture, createRun, assert, assertEqual } = require("./lib/harness");
const F = require("./lib/fixtures");

/// Render `snapshot` into `#messages` with the options `main.js` passes, at `now`, and return
/// what a person reads: every duration label, and the whole text of the conversation.
function renderSnapshot(page, snapshot, now) {
  return page.evaluate(
    ({ snapshot, now }) => {
      const model = window.RichTimeline.createModel();
      window.RichTimeline.bind(model, snapshot.entityId, snapshot.threadId, 1);
      window.RichTimeline.applySnapshot(model, snapshot);
      const container = document.getElementById("messages");
      window.RichTimeline.render(model, container, {
        now,
        expandedMessages: new Set(),
        avatarAlreadyShown: true,
        isExpanded: () => false,
        toggle: () => {},
        rerender: () => {},
        copy: () => {},
        retry: () => {},
        openWorker: () => {},
      });
      window.RichTimeline.updateTimers(model, container, now);
      return {
        rows: Array.from(container.querySelectorAll(".tl-duration")).map((n) => n.innerText.trim()),
        labels: Array.from(container.querySelectorAll(".tl-duration-label")).map((n) => n.textContent),
        text: container.innerText,
      };
    },
    { snapshot, now }
  );
}

/// One message Rich sent on his own, as `get_timeline` serializes it: a proactive turn has no
/// CEO prompt, its prose carries `phase: "proactive"`, and its turn ran for no measured time.
function proactiveTurn(turnId, text, activeMs) {
  const base = {
    entityId: "northwind",
    threadId: "thr_fem",
    turnId,
    bindingRevision: 1,
    createdAt: 1787949100000,
    visibility: "ceo",
  };
  return [
    Object.assign({}, base, { id: turnId + ":text:0", sequence: 0, slot: "stream", kind: "rich_message", phase: "proactive", text }),
    Object.assign({}, base, {
      id: turnId + ":duration",
      sequence: null,
      slot: "terminal",
      kind: "work_duration",
      state: "completed",
      startedAt: null,
      endedAt: 1787949100000,
      activeMs,
    }),
  ];
}

async function main() {
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();
  const run = createRun("the conversation while Rich works — no row that just says Worked");
  const page = await openFixture(browser);

  // =====================================================================================
  // FINDING 13 — no row that just says "Worked"
  // =====================================================================================

  await run.check('13  a message Rich sent on his own has no "Worked" row above it', async () => {
    const r = await renderSnapshot(
      page,
      F.snapshot([
        F.userMessage(),
        F.richMessage(0, "Here are three subject lines.", 0),
        F.duration("completed", 15000),
        // The two shapes a proactive turn's duration comes in: never measured, and measured
        // under the one-second display floor.
        ...proactiveTurn("turn_alert", "Usage is climbing fast.", null),
        ...proactiveTurn("turn_normal", "Usage is back to normal.", 0),
      ]),
      1787949200000
    );
    assert(r.text.includes("Usage is climbing fast.") && r.text.includes("Usage is back to normal."), "the messages render: " + r.text);
    assertEqual(r.labels, ["Worked for 15s"], "the one measured turn keeps its row, and nothing else has one");
    assert(!r.rows.some((row) => /^worked$/i.test(row)), 'a row reads only "Worked": ' + JSON.stringify(r.rows));
    return "rows: " + JSON.stringify(r.rows);
  });

  await run.check('13  a turn with work to show but no measured time names what it opens, never "Worked"', async () => {
    const r = await renderSnapshot(
      page,
      F.snapshot([
        F.userMessage(),
        F.activity("m_read", "Read a file", 0),
        F.richMessage(0, "Done.", 1),
        F.duration("completed", null),
      ]),
      1787949200000
    );
    assertEqual(r.labels, ["What Rich did"], "the disclosure's label");
    return "label: " + r.labels[0];
  });

  await run.check('13  an answered turn still working keeps "What Rich did" when its timer ticks', async () => {
    // The handover review of 742df82c1 (P3): `renderDurationRow` names the disclosure "What Rich
    // did" when the turn has work to show and no number, but `updateTimers` wrote
    // `durationRow`'s empty label back over it on the next tick while the turn was still live:
    // once Rich has answered, the band's second timer is gone (finding 22), so that is every
    // answered turn that keeps working.
    const r = await page.evaluate((snapshot) => {
      const T = window.RichTimeline;
      const model = T.createModel();
      T.bind(model, snapshot.entityId, snapshot.threadId, 1);
      T.applySnapshot(model, snapshot);
      const fence = { entityId: snapshot.entityId, threadId: snapshot.threadId, bindingRevision: 1, turnId: "turn_ok", visibility: "ceo" };
      const at = 1787949200000;
      T.onTurnStatus(model, Object.assign({}, fence, { status: "working", startedAt: at - 9000, at: at - 9000 }));
      T.onTurnStatus(model, Object.assign({}, fence, { status: "working", answered: true, at }));
      const container = document.getElementById("messages");
      T.render(model, container, {
        now: at,
        expandedMessages: new Set(),
        avatarAlreadyShown: true,
        isExpanded: () => false,
        toggle: () => {},
        rerender: () => {},
        copy: () => {},
        retry: () => {},
        openWorker: () => {},
      });
      const label = () => Array.from(container.querySelectorAll(".tl-duration-label")).map((n) => n.textContent);
      const before = label();
      T.updateTimers(model, container, at + 1000);
      const btn = container.querySelector(".tl-duration-btn");
      return { before, after: label(), aria: btn ? btn.getAttribute("aria-label") : null };
    }, F.snapshot([F.userMessage(), F.activity("m_read", "Read a file", 0), F.richMessage(0, "Here it is.", 1)]));
    assertEqual(r.before, ["What Rich did"], "drawn with the disclosure's name");
    assertEqual(r.after, ["What Rich did"], "and a timer tick keeps it");
    assert(r.aria && r.aria.startsWith("What Rich did"), "its accessible name starts with the same words: " + r.aria);
    return "before " + JSON.stringify(r.before) + ", after a tick " + JSON.stringify(r.after);
  });

  await run.check("the page rendered all of that without a single script error", async () => {
    assertEqual(JSON.stringify(page.__errors), "[]", "page errors");
    return "no page errors, no console errors";
  });

  await page.close();
  await browser.close();
  return run.report();
}

main().then(
  (failed) => { process.exitCode = failed ? 1 : 0; },
  (e) => {
    console.error(e);
    process.exit(1);
  }
);

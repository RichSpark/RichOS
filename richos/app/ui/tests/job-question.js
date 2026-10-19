// A BACKGROUND JOB'S QUESTION IS NOT A TURN THAT A QUIT CUT OFF.
//
// Ray's walk of nightly candidate 38, D8 (`docs/verification/2026-10-05-nightly-38-candidate-walk.md`,
// frame `e-overlap-light`): a background job stopped to ask him a question, and under that
// question card the conversation said
//
//     This turn was still running the last time RichOS was open and I can't tell you how
//     it ended.
//
// while RichOS had never closed.
//
// THE CAUSE. A job's question is scoped to its OBLIGATION, not to a conversation turn
// (`native.rs`, `prepare_question_scope(&work.entity_id, &work.thread_id, &work.obligation_id, ...)`),
// and `question_host.rs` appends it to the timeline snapshot with that id as its `turnId`.
// No ledger turn has that id, so the snapshot carries no `work_duration` row for it. The
// renderer nonetheless made a turn RECORD for it out of the question item alone, with the
// record's created defaults (`queued`, not live) — exactly what a turn left running by a quit
// looks like — and drew "Status unavailable" and the quit card under the question.
//
// WHAT THIS SUITE PINS, each absence beside a positive control in the same page:
//   1. A job's question renders, with no duration row and no quit card.
//   2. A ledger turn that really was left running still gets the quit card (the §14 case
//      the card exists for), so check 1's absence is a fact about the question.
//   3. The question keeps its place in the conversation, between the turns around it.
//
// Run: node job-question.js   (or `npm test` for every suite in this directory)

"use strict";

const { loadPlaywright, openFixture, createRun, assert, assertEqual } = require("./lib/harness");

const ENTITY = "northwind";
const THREAD = "thr_jobq";
const OBLIGATION = "obl_7f3c2a";
const T0 = 1788000000000;

function base(id, turnId, createdAt, extra) {
  return Object.assign(
    {
      id,
      entityId: ENTITY,
      threadId: THREAD,
      turnId,
      bindingRevision: 1,
      createdAt,
      visibility: "ceo",
      slot: "stream",
      sequence: null,
    },
    extra || {}
  );
}

/// One ledger turn as `Timeline::project` emits it: his words, Rich's answer, the duration row.
function ledgerTurn(turnId, at, state) {
  const ended = state === "completed";
  return [
    base(turnId + ":user", turnId, at, { kind: "user_message", slot: "opening", text: "Add the notes to Acme." , source: "text" }),
    base(turnId + ":text:0", turnId, at + 500, { kind: "rich_message", phase: "unknown", sequence: 0, text: "On it!" }),
    base(turnId + ":duration", turnId, at, {
      kind: "work_duration",
      slot: "terminal",
      state,
      startedAt: at + 100,
      endedAt: ended ? at + 2100 : null,
      activeMs: ended ? 2000 : null,
    }),
  ];
}

/// The item `questions.rs` `Question::item` writes for a job's question: its `turnId` is the
/// job's obligation, which no ledger turn carries.
function jobQuestion(at) {
  return base("q_jobq", OBLIGATION, at, {
    kind: "question",
    sequence: at,
    text: "The checked notes can't be added to Acme yet. What should I do with panel-check.md?",
    question: {
      id: "q_jobq",
      thread_id: THREAD,
      text: "What should I do with panel-check.md?",
      options: [
        { id: "keep", label: "Keep it as part of Acme", description: "Save it for good." },
        { id: "move", label: "Move it out of Acme", description: "Nothing is lost." },
      ],
      multiple: false,
      free_answer: true,
      state: "open",
      revision: 0,
    },
  });
}

function snapshot(items) {
  return { entityId: ENTITY, threadId: THREAD, mode: "ceo", bindingRevision: 1, items };
}

/// The fixture page loads `timeline.js` alone. `questions.js` draws the real card in the app;
/// here a marked stand-in is enough, because what is under test is the turn AROUND it.
const STUB_QUESTIONS = () => {
  window.RichQuestions = {
    render(item) {
      const el = document.createElement("div");
      el.className = "question-row";
      el.dataset.questionId = item.question.id;
      el.textContent = item.question.text;
      return el;
    },
  };
};

const readPage = (page) =>
  page.evaluate(() => ({
    quitCards: Array.from(document.querySelectorAll(".tl-intervention")).map((n) => n.textContent),
    unavailable: document.body.textContent.includes("Status unavailable"),
    questions: Array.from(document.querySelectorAll(".question-row")).map((n) => n.dataset.questionId),
    sections: Array.from(document.querySelectorAll("section.tl-turn")).map((s) => s.dataset.turnId),
  }));

async function main() {
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();
  const run = createRun("D8 — a background job's question is not a turn cut off by a quit");
  const page = await openFixture(browser);
  await page.evaluate(STUB_QUESTIONS);

  await run.check("a job's question renders with no duration row and no quit card", async () => {
    await page.evaluate(
      (s) => window.__render(s),
      snapshot([...ledgerTurn("turn_a", T0, "completed"), jobQuestion(T0 + 60000)])
    );
    const seen = await readPage(page);
    assertEqual(seen.questions, ["q_jobq"], "the job's question must be on screen");
    assertEqual(seen.quitCards, [], "a question from a job that is still open is not a turn a quit cut off");
    assert(!seen.unavailable, "no 'Status unavailable' row for a turn the ledger never had");
    const record = await page.evaluate(
      (id) => (window.RichTimeline.turnsOf(window.__model).find((t) => t.turnId === id) || {}).record || null,
      OBLIGATION
    );
    assertEqual(record, null, "the obligation is not a ledger turn, so it has no turn record");
    return "question drawn; 0 quit cards; no status row";
  });

  await run.check("POSITIVE CONTROL: a ledger turn left running by a quit still gets the card", async () => {
    await page.evaluate(
      (s) => window.__render(s),
      snapshot([...ledgerTurn("turn_a", T0, "working"), jobQuestion(T0 + 60000)])
    );
    const seen = await readPage(page);
    assertEqual(seen.quitCards.length, 1, "the §14 card for a turn really left running");
    assert(
      seen.quitCards[0].includes("last time RichOS was open"),
      "it is the quit card: " + seen.quitCards[0]
    );
    assertEqual(seen.questions, ["q_jobq"], "and the question is still drawn");
    return "1 quit card, on turn_a only";
  });

  await run.check("the question keeps its place between the turns around it", async () => {
    await page.evaluate(
      (s) => window.__render(s),
      snapshot([
        ...ledgerTurn("turn_a", T0, "completed"),
        jobQuestion(T0 + 60000),
        ...ledgerTurn("turn_b", T0 + 120000, "completed"),
      ].sort((x, y) => x.createdAt - y.createdAt))
    );
    const seen = await readPage(page);
    assertEqual(seen.sections, ["turn_a", OBLIGATION, "turn_b"], "conversation order");
    assertEqual(seen.quitCards, [], "no quit card anywhere");
    return seen.sections.join(" → ");
  });

  await run.check("no page errors", async () => {
    assertEqual(page.__errors, [], "page errors");
    return "none";
  });

  await browser.close();
  process.exit(run.report() > 0 ? 1 : 0);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

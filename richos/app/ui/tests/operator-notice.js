// HIS TEAM, ON THE CONVERSATION'S TIMELINE — the operator notice surface.
//
// The operator-client record's §7 item 1 (richos-hq
// docs/verification/2026-09-25-operator-client/README.md): the operator notice kinds
// (update, question, answer, outcome, failed, alarm, team) on the conversation's timeline, WCAG
// AA in both themes. The seam was built before the surface and is not changed by it:
// `take_operator_notices` reads AND marks `<app data>/operator/<entity>/<thread>/notices.jsonl`,
// and `rich://operator-notice` only wakes that read (`app/STREAMING.md`, "His team, on an
// operator install"). Only an install whose `operator.json` passed the gate has a desk; on any
// other the command answers an empty list and the event is never emitted.
//
// WHAT THIS SUITE PINS, each negative beside a positive control that proves it could fail:
//
//   1. Every kind draws the notice card with a from-line saying who it is from and what kind
//      of line it is, the words verbatim, and none of Rich's identity or his Copy control.
//   2. The attribution is only claimed where it is true: `team` is RichOS speaking about his
//      team and `alarm` is his engine, so neither says "Your team"; an `outcome` never says
//      done or finished; an unknown kind is drawn, as "About your team".
//   3. The durable half: notices said while the window was not listening are drawn when the
//      conversation opens, where they happened in his scrollback, marked told, and never drawn
//      twice. The assignment is named by its title, never its identifier.
//   4. The live half: a pushed notice is drawn; one pushed while a turn runs is HELD and drawn
//      at the boundary, never mid-sentence and never dropped.
//   5. Legacy work prose cannot become a conversation card. Genuine operator reports keep
//      their attribution; two assignments that say the same sentence are still two cards.
//   6. On any other install the operator read is empty and no team card is drawn. Legacy
//      work prose cannot fabricate a status card there either.
//   7. It survives his next sentence (the snapshot reload), attribution included.
//   8. CONTRAST, computed from WebKit's resolved colors, for every kind in BOTH themes: the
//      from-line and the body at 4.5:1 and at least 16px, the left rule at 3:1.

"use strict";

const fs = require("fs");
const path = require("path");
const {
  loadPlaywright, openFixture, createRun, assert, assertEqual, UI_DIR, leaveHome, openThread,
} = require("./lib/harness");
const F = require("./lib/fixtures");

const APP = "file://" + path.join(UI_DIR, "index.html");
const KINDS = ["update", "question", "answer", "outcome", "failed", "alarm", "team"];

/// What each kind's from-line reads, as a person reads it. Typed here, not imported from
/// `timeline.js`: a suite that read the expected words out of the file under test would pass
/// over any change to them.
const FROM_LINE = {
  update: "Your team",
  question: "Your team asks you",
  answer: "Your team answered",
  outcome: "Your team reported back",
  failed: "Your team could not finish",
  alarm: "Alarm",
  team: "About your team",
};
const DANGER = new Set(["failed", "alarm"]);

/// Sentences shaped like the ones the backend says for each kind (`operator_host.rs`,
/// `operator_desk.rs`): a lead's own words for the first five, RichOS's for `team`, the
/// engine's for `alarm`.
const SAID = {
  update: "Pulled the three branches and started the rebase.",
  question: "Should the release notes mention the pricing change?",
  answer: "Yes, 42 of them.",
  outcome: "Landed the pricing page.\n\nLanded and pushed 3f2a1c0 in richos.",
  failed: "It cannot be done: the remote refuses the push.",
  alarm: "The nightly build has been waiting on its lock for 40 minutes.",
  team: "Stopped w2-sonnet-a.",
};

function ratioHelper() {
  return `
    window.__ratio = (node, prop) => {
      if (!node) throw new Error("no node");
      const parse = (value) => {
        const n = value.match(/[\\d.]+/g).map(Number);
        return { r: n[0], g: n[1], b: n[2], a: n.length > 3 ? n[3] : 1 };
      };
      let background = null;
      for (let el = node.parentElement; el; el = el.parentElement) {
        const c = parse(getComputedStyle(el).backgroundColor);
        if (c.a === 1) { background = c; break; }
      }
      if (!background) background = { r: 255, g: 255, b: 255, a: 1 };
      const own = parse(getComputedStyle(node).backgroundColor);
      if (own.a === 1) background = own;
      const fg = parse(getComputedStyle(node)[prop || "color"]);
      const flat = {
        r: fg.r * fg.a + background.r * (1 - fg.a),
        g: fg.g * fg.a + background.g * (1 - fg.a),
        b: fg.b * fg.a + background.b * (1 - fg.a),
      };
      const channel = (v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4); };
      const lum = (c) => 0.2126 * channel(c.r) + 0.7152 * channel(c.g) + 0.0722 * channel(c.b);
      const a = lum(flat), b = lum(background);
      return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
    };
  `;
}

/// One real Rich message (the positive control for every absence) and one card per kind, in
/// `theme`, on the shipping renderer.
async function everyKind(browser, theme) {
  const page = await openFixture(browser);
  await page.addScriptTag({ content: ratioHelper() });
  await page.evaluate(
    ([snapshot, kinds, said, mode]) => {
      document.documentElement.dataset.theme = mode;
      window.__render(snapshot, {});
      let at = 1787950000000;
      for (const kind of kinds) {
        window.RichTimeline.addLocalNotice(window.__model, said[kind], at++, {
          kind,
          about: kind === "team" || kind === "alarm" ? null : "The pricing page",
        });
      }
      window.__renderOnly();
    },
    [
      { entityId: "northwind", threadId: "thr_fem", mode: "ceo", bindingRevision: 1,
        items: [F.richMessage(0, "Pulling the comparables now.", 0)] },
      KINDS, SAID, theme,
    ]
  );
  return page;
}

/// Read one card's from-line and body as a person reads them: visible text only, the
/// screen-reader-only "about " left out and the separator as drawn.
function readCard(page, kind) {
  return page.evaluate((k) => {
    const card = document.querySelector(`.tl-notice[data-team-kind="${k}"]`);
    if (!card) return null;
    const from = card.querySelector(".tl-notice-from");
    const visible = (node) =>
      Array.from(node.childNodes)
        .filter((n) => !(n.classList && n.classList.contains("sr-only")))
        .map((n) => n.textContent)
        .join("");
    return {
      from: from ? visible(from) : null,
      spoken: from ? from.textContent : null,
      body: (card.querySelector(".tl-notice-body") || {}).innerText || "",
      danger: card.classList.contains("tl-notice--danger"),
      whos: card.querySelectorAll(".tl-rich-meta, .tl-avatar").length,
      buttons: card.querySelectorAll("button").length,
      srStatus: Array.from(card.querySelectorAll(".sr-only")).map((n) => n.textContent),
    };
  }, kind);
}

async function openApp(browser, preset) {
  const page = await browser.newPage({ viewport: { width: 1400, height: 950 } });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push("console: " + m.text());
  });
  await page.addInitScript((v) => { window.__RICHOS_MOCK_PRESET__ = v; }, preset || {});
  await page.goto(APP);
  await leaveHome(page);
  page.__errors = errors;
  return page;
}

/// The team cards on screen, in scrollback order, as `kind|from-line|body`.
function teamCards(page) {
  return page.evaluate(() =>
    Array.from(document.querySelectorAll("#messages .tl-notice--team")).map((card) => {
      const from = card.querySelector(".tl-notice-from");
      const visible = Array.from(from.childNodes)
        .filter((n) => !(n.classList && n.classList.contains("sr-only")))
        .map((n) => n.textContent)
        .join("");
      return card.dataset.teamKind + "|" + visible + "|" + card.querySelector(".tl-notice-body").innerText.trim();
    })
  );
}

const ACME_ROWS = [
  { id: "asg_7f3c9e", title: "The Acme contract draft", kind: "task", turnRef: "ledger:acme:t0",
    state: "running", detail: "", repositories: [], registeredAtMs: 1, canStop: true,
    onTheConnection: false },
];

async function main() {
  const run = createRun("his team's notices on the conversation's timeline (operator install)");
  const browser = await loadPlaywright().webkit.launch();

  // ---- 1 and 2: the card, per kind ------------------------------------------------------
  await run.check("every kind draws the notice card with who it is from, and none of Rich's identity", async () => {
    const page = await everyKind(browser, "dark");
    const out = [];
    for (const kind of KINDS) {
      const card = await readCard(page, kind);
      assert(card, `the ${kind} notice did not render at all`);
      assertEqual(card.from, FROM_LINE[kind] + (kind === "team" || kind === "alarm" ? "" : " · The pricing page"),
        `the ${kind} from-line`);
      assertEqual(card.whos, 0, `the ${kind} notice carries Rich's byline or avatar`);
      assertEqual(card.buttons, 0, `the ${kind} notice offers a control (Copy is for what Rich wrote)`);
      assertEqual(card.srStatus.includes("RichOS status"), false,
        `the ${kind} notice is ALSO named "RichOS status", a second name that contradicts its from-line`);
      assertEqual(card.danger, DANGER.has(kind), `the ${kind} notice's tone`);
      out.push(kind);
    }
    // THE WORDS ARE HIS TEAM'S, VERBATIM — the body is the notice's text, blank lines and all.
    const outcome = await readCard(page, "outcome");
    assertEqual(outcome.body.replace(/\n+/g, "\n").trim(), SAID.outcome.replace(/\n+/g, "\n"), "the outcome's words");
    // A screen reader hears the title as "about <title>", not as a bare dot.
    const question = await readCard(page, "question");
    assertEqual(question.spoken.replace(/\s+/g, " ").trim(), "Your team asks you · about The pricing page",
      "what a screen reader hears for the question's from-line");
    // POSITIVE CONTROL on the same page: a real Rich message has the byline and Copy the
    // notices lack, so those absences are facts about the notices.
    assertEqual(await page.locator("article.tl-rich .tl-who").count(), 1, "the control message has no byline");
    assertEqual(await page.locator("article.tl-rich button.tl-mini-btn").count(), 1, "the control message has no Copy");
    // AND A PLAIN LOCAL NOTICE IS UNCHANGED: no from-line, still "RichOS status".
    await page.evaluate(() => {
      window.RichTimeline.addLocalNotice(window.__model, "I can't hear anything.", 1787950009999);
      window.__renderOnly();
    });
    const plain = page.locator(".tl-notice:not(.tl-notice--team)");
    assertEqual(await plain.count(), 1, "the app's own status line did not render");
    assertEqual(await plain.locator(".tl-notice-from").count(), 0, "the app's own status line grew a from-line");
    assertEqual((await plain.locator(".sr-only").first().innerText()).trim(), "RichOS status", "its accessible name");
    assertEqual(page.__errors.length, 0, "renderer errors: " + page.__errors.join(" | "));
    await page.close();
    return `${out.length} kinds, each attributed, 0 bylines, 0 controls; the Rich message beside them has both`;
  });

  await run.check("attribution is only claimed where it is true, and nothing is called done", async () => {
    const page = await everyKind(browser, "dark");
    const team = await readCard(page, "team");
    const alarm = await readCard(page, "alarm");
    assert(!/^Your team\b/.test(team.from), `RichOS's line about his team is attributed to his team: "${team.from}"`);
    assert(!/^Your team\b/.test(alarm.from), `his engine's alarm is attributed to his team: "${alarm.from}"`);
    for (const kind of ["outcome", "answer", "update"]) {
      const card = await readCard(page, kind);
      assert(!/\b(done|finished|complete)/i.test(card.from), `the ${kind} from-line claims completion: "${card.from}"`);
    }
    // A NEWER BACKEND'S KIND is drawn, and not as his team's words.
    await page.evaluate(() => {
      window.RichTimeline.addLocalNotice(window.__model, "Something new.", 1787950020000, { kind: "handover", about: null });
      window.__renderOnly();
    });
    const unknown = await readCard(page, "handover");
    assert(unknown, "a notice of an unknown kind was dropped");
    assertEqual(unknown.from, "About your team", "an unknown kind's from-line");
    // POSITIVE CONTROL: the five lead kinds DO say "Your team", so the two refusals above are
    // about those kinds and not about a from-line that never says it.
    for (const kind of ["update", "question", "answer", "outcome", "failed"]) {
      assert(/^Your team\b/.test((await readCard(page, kind)).from), `the ${kind} line is not attributed to his team`);
    }
    await page.close();
    return `team: "${team.from}"; alarm: "${alarm.from}"; outcome: "${FROM_LINE.outcome}"; unknown: "About your team"`;
  });

  // ---- 3: the durable half, in the shell --------------------------------------------------
  await run.check("what his team said while he was away is drawn when the conversation opens, once, in place", async () => {
    const page = await openApp(browser, { operator: true, assignments: { acme: ACME_ROWS } });
    await openThread(page, "hiring");
    // Said while he was on another conversation: on disk only, as a closed window finds it.
    // Timed between acme's two seeded turns (20 h and 30 min ago in `mock.js`) so the check
    // can see WHERE it lands: under the newest answer would say "this just happened".
    const at = Date.now() - 1000 * 60 * 60 * 2;
    await page.evaluate((t) => {
      window.__RICHOS_MOCK__.operatorSay("acme", { kind: "question", text: "Should the draft keep the 90-day term?", handle: "asg_7f3c9e", at_ms: t }, { push: false });
      window.__RICHOS_MOCK__.operatorSay("acme", { kind: "team", text: "Stopped w2-sonnet-a.", handle: null, at_ms: t + 1000 }, { push: false });
    }, at);
    assertEqual(await page.evaluate(() => window.__RICHOS_MOCK__.operatorUndelivered("acme")), 2, "precondition: two notices on disk");
    await openThread(page, "acme");
    await page.waitForFunction(() => document.querySelectorAll("#messages .tl-notice--team").length === 2, undefined, { timeout: 8000 });
    const cards = await teamCards(page);
    assertEqual(cards, [
      "question|Your team asks you · The Acme contract draft|Should the draft keep the 90-day term?",
      "team|About your team|Stopped w2-sonnet-a.",
    ], "the two notices as drawn");
    // NEVER THE IDENTIFIER: the handle is a key, and the title is what he reads.
    const text = await page.evaluate(() => document.getElementById("messages").innerText);
    assert(!text.includes("asg_7f3c9e"), "the assignment's identifier reached his screen");
    assertEqual(await page.evaluate(() => window.__RICHOS_MOCK__.operatorUndelivered("acme")), 0, "the read did not mark them told");
    // WHERE IT HAPPENED: after the first turn, before the second.
    const order = await page.evaluate(() =>
      Array.from(document.querySelectorAll("#messages .tl-turn")).map((turn) =>
        turn.querySelector(".tl-notice--team") ? "team" : turn.querySelector(".tl-notice") ? "notice" : "turn"
      )
    );
    const firstTeam = order.indexOf("team");
    assert(firstTeam > 0 && order.slice(firstTeam + 2).includes("turn"),
      "the notices are not between the two answers they came between: " + order.join(","));
    // ONCE: leaving and coming back reads the store again, and it has nothing new.
    await openThread(page, "hiring");
    const callsBefore = await page.evaluate(() => window.__RICHOS_MOCK__.operatorCalls().filter((t) => t === "acme").length);
    await openThread(page, "acme");
    // The second opening's durable read, and the thread drawn after it, are the facts waited
    // for (hang guard); this used to be a 300ms sleep (2026-09-29, audit R10). Both assertions
    // below still name a read that never ran or a count that is wrong.
    await page
      .waitForFunction(
        (n) => window.__RICHOS_MOCK__.operatorCalls().filter((t) => t === "acme").length > n,
        callsBefore,
        { timeout: 10000 }
      )
      .catch(() => {});
    await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
    assertEqual((await teamCards(page)).length, 2, "coming back drew the notices again (or lost them)");
    const asked = await page.evaluate(() => window.__RICHOS_MOCK__.operatorCalls().filter((t) => t === "acme").length);
    assert(asked >= 2, `the durable read ran ${asked} time(s) on acme; the second opening never asked`);
    assertEqual(page.__errors.length, 0, "shell errors: " + page.__errors.join(" | "));
    await page.close();
    return `2 notices said while away: drawn on opening, between the two answers, titled, marked told; re-opened ${asked}x and still 2`;
  });

  // ---- 4: the live half, and the hold ----------------------------------------------------
  await run.check("a pushed notice is drawn; one pushed mid-turn waits for the boundary and is never dropped", async () => {
    const page = await openApp(browser, { operator: true, assignments: { acme: ACME_ROWS } });
    await openThread(page, "acme");
    await page.evaluate(() =>
      window.__RICHOS_MOCK__.operatorSay("acme", { kind: "update", text: "Started on the draft.", handle: "asg_7f3c9e" })
    );
    await page.waitForFunction(() => document.querySelectorAll("#messages .tl-notice--team").length === 1, undefined, { timeout: 5000 });
    // His turn starts; his team speaks while Rich is mid-sentence.
    await page.fill("#input", "how is the draft going?");
    await page.press("#input", "Enter");
    await page.waitForFunction(() => anyLiveTurn(), undefined, { timeout: 5000 }); // eslint-disable-line no-undef
    // A real operator question is held; legacy app-written work prose is excluded.
    await page.evaluate(async () => {
      window.__RICHOS_MOCK__.operatorSay("acme", { kind: "question", text: "Should the draft keep the 90-day term?", handle: "asg_7f3c9e" });
      window.__RICHOS_MOCK__.workNoticeQueue("acme", { assignmentId: "a9", title: "x", kind: "failed", text: "The nightly build stopped before it finished.", raisedAtMs: Date.now() });
      await drainWorkNotices(); // eslint-disable-line no-undef
    });
    // Two of the page's own frames — the shell renders at most once per frame (§15) — rather
    // than 250ms of this process's time (audit R10): a notice that was NOT held would be on
    // screen by then on any host, and the turn has had no wall-clock sleep in which to end.
    await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
    const during = await page.evaluate(() => ({
      live: anyLiveTurn(), // eslint-disable-line no-undef
      cards: document.querySelectorAll("#messages .tl-notice--team").length,
      plain: document.querySelectorAll("#messages .tl-notice:not(.tl-notice--team)").length,
      told: window.__RICHOS_MOCK__.operatorUndelivered("acme"),
    }));
    assert(during.live, "precondition: the turn was over before the notice arrived, so this proves nothing about holding");
    assertEqual(during.cards, 1, "his team's notice was drawn mid-sentence");
    assertEqual(during.plain, 0, "the background result was drawn mid-sentence");
    assertEqual(during.told, 0, "the read did not take it, so the hold is not what kept it off the screen");
    // THE BOUNDARY: the turn ends and the held operator notice is drawn here. The
    // mock has no later one, so a hold that waited for the next turn would time out here,
    // which is how this check found that the shipped flush never fired at a turn's end.
    await page.waitForFunction(() => !anyLiveTurn(), undefined, { timeout: 15000 }); // eslint-disable-line no-undef
    await page.waitForFunction(
      () => document.querySelectorAll("#messages .tl-notice--team").length === 2,
      undefined, { timeout: 8000 }
    );
    assertEqual(await page.locator("#messages .tl-notice:not(.tl-notice--team)").count(), 0,
      "legacy app-written work prose became a conversation card at the boundary");
    const cards = await teamCards(page);
    assertEqual(cards[1], "question|Your team asks you · The Acme contract draft|Should the draft keep the 90-day term?",
      "the held notice as drawn at the boundary");
    assertEqual(page.__errors.length, 0, "shell errors: " + page.__errors.join(" | "));
    await page.close();
    return "operator push drawn at once; mid-turn question held and drawn at the boundary; legacy work prose never drawn";
  });

  // ---- 5: one report, one card -------------------------------------------------------------
  await run.check("legacy work prose is excluded; operator reports keep attribution, separation and hold", async () => {
    // Driven through the SHIPPING functions, in a context of their own — the same method
    // `background-work.js` uses for the work lane — because the order of two drains is
    // exactly what a whole shell cannot be made to hold still.
    const vm = require("node:vm");
    const src = fs.readFileSync(path.join(UI_DIR, "main.js"), "utf8");
    const lanes = { work: [], team: [] };
    const context = vm.createContext({ window: {}, console, Date, Map, Set, Promise,
      activeThreadId: "a", followBottom: false, scheduleRender() {}, busy: false,
      assignments: { rows: [{ id: "one", title: "The pricing page" }] },
      Bridge: { listen() {}, async invoke(name) {
        if (name === "take_work_notices") { const r = lanes.work; lanes.work = []; return r; }
        if (name === "take_operator_notices") { const r = lanes.team; lanes.team = []; return r; }
        if (name === "get_assignments") return { assignments: [] };
        throw new Error("unexpected command " + name);
      } } });
    vm.runInContext(fs.readFileSync(path.join(UI_DIR, "timeline.js"), "utf8"), context);
    vm.runInContext(`
      let timelineModel = window.RichTimeline.createModel();
      window.RichTimeline.bind(timelineModel, "company", "a", 1);
      function anyLiveTurn() { return busy; }
    `, context);
    vm.runInContext(src.slice(src.indexOf("let voiceBusy = false;"), src.indexOf("/// A line Rich says LOCALLY")), context);
    const words = "Landed the pricing page.\n\nLanded and pushed 3f2a1c0 in richos.";
    const cards = () => vm.runInContext(`Array.from(timelineModel.items.values())
      .filter((i) => i.kind === "local_notice").map((i) => (i.team ? i.team.kind + ":" + (i.team.about || "") : "app") + "|" + i.text)`, context);

    // Legacy work prose first: it is excluded before the real operator report arrives.
    lanes.work = [{ assignmentId: "one", kind: "settled", text: words, raisedAtMs: 100 }];
    await vm.runInContext("drainWorkNotices()", context);
    assertEqual(cards().length, 0, "legacy work prose was presented as a conversation card");
    lanes.team = [{ at_ms: 101, handle: "one", kind: "outcome", text: words, delivered_at_ms: null }];
    await vm.runInContext("drainOperatorNotices()", context);
    const workFirst = cards();
    assertEqual(workFirst, ["outcome:The pricing page|" + words], "work lane first: not one attributed card");

    // TEAM FIRST, on a fresh conversation model.
    vm.runInContext(`timelineModel = window.RichTimeline.createModel();
      window.RichTimeline.bind(timelineModel, "company", "a", 1);`, context);
    lanes.team = [{ at_ms: 200, handle: "one", kind: "outcome", text: words, delivered_at_ms: null }];
    await vm.runInContext("drainOperatorNotices()", context);
    lanes.work = [{ assignmentId: "one", kind: "settled", text: words, raisedAtMs: 199 }];
    await vm.runInContext("drainWorkNotices()", context);
    assertEqual(cards(), ["outcome:The pricing page|" + words], "team lane first: not one attributed card");

    // TWO ASSIGNMENTS, THE SAME SENTENCE: two cards. And a line with no assignment is never
    // merged with anything.
    lanes.team = [
      { at_ms: 300, handle: "two", kind: "failed", text: "It cannot be done.", delivered_at_ms: null },
      { at_ms: 301, handle: "three", kind: "failed", text: "It cannot be done.", delivered_at_ms: null },
      { at_ms: 302, handle: null, kind: "team", text: "Stopped w2-sonnet-a.", delivered_at_ms: null },
      { at_ms: 303, handle: null, kind: "team", text: "Stopped w2-sonnet-a.", delivered_at_ms: null },
    ];
    await vm.runInContext("drainOperatorNotices()", context);
    assertEqual(cards().length, 5, "distinct notices were merged: " + JSON.stringify(cards()));

    // The operator report is still held, while legacy work prose remains excluded.
    vm.runInContext(`timelineModel = window.RichTimeline.createModel();
      window.RichTimeline.bind(timelineModel, "company", "a", 1); busy = true;`, context);
    lanes.work = [{ assignmentId: "four", kind: "failed", text: "Nothing landed.", raisedAtMs: 400 }];
    lanes.team = [{ at_ms: 401, handle: "four", kind: "failed", text: "Nothing landed.", delivered_at_ms: null }];
    await vm.runInContext("drainWorkNotices()", context);
    await vm.runInContext("drainOperatorNotices()", context);
    assertEqual(cards().length, 0, "drawn mid-turn");
    vm.runInContext("busy = false; flushWorkNotices()", context);
    assertEqual(cards(), ["failed:|Nothing landed."], "held pair at the boundary");
    return "legacy prose: 0 cards; operator reports retain attribution in either order, distinct assignments stay separate and the held report appears at the boundary";
  });

  // ---- 6: every other install --------------------------------------------------------------
  await run.check("on any other install nothing changes: the read is empty and no team card is drawn", async () => {
    // POSITIVE CONTROL FIRST, same driver and same words: an operator install draws it.
    const control = await openApp(browser, { operator: true });
    await openThread(control, "hiring");
    await control.evaluate(() =>
      window.__RICHOS_MOCK__.operatorSay("acme", { kind: "update", text: "Started on the draft.", handle: null }, { push: false })
    );
    await openThread(control, "acme");
    await control.waitForFunction(() => document.querySelectorAll("#messages .tl-notice--team").length === 1, undefined, { timeout: 8000 });
    await control.close();

    const page = await openApp(browser, {});
    await openThread(page, "hiring");
    await page.evaluate(() => {
      window.__RICHOS_MOCK__.operatorSay("acme", { kind: "update", text: "Started on the draft.", handle: null }, { push: false });
      window.__RICHOS_MOCK__.operatorSay("acme", { kind: "question", text: "Pushed, on a product install?", handle: null });
      window.__RICHOS_MOCK__.workNoticeQueue("acme", { assignmentId: "a9", title: "x", kind: "failed", text: "The nightly build stopped before it finished.", raisedAtMs: Date.now() });
    });
    await openThread(page, "acme");
    await page.waitForFunction(() => window.__RICHOS_MOCK__.operatorCalls().includes("acme"), undefined, { timeout: 8000 });
    await page.evaluate(async () => { await drainWorkNotices(); }); // eslint-disable-line no-undef
    await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
    assertEqual(await page.locator("#messages .tl-notice--team").count(), 0, "a product install drew his team's notice");
    assertEqual(await page.locator("#messages .tl-notice-from").count(), 0, "a product install drew a from-line");
    assertEqual(await page.locator("#messages .tl-notice").count(), 0,
      "legacy app-written work prose became a status card on a product install");
    assertEqual(page.__errors.length, 0, "shell errors: " + page.__errors.join(" | "));
    await page.close();
    return "operator control: 1 team card; product install: read completed, 0 team cards, 0 from-lines and no fabricated status card";
  });

  // ---- 7: his next sentence ------------------------------------------------------------------
  await run.check("an attributed notice survives the reload his next sentence causes", async () => {
    const page = await everyKind(browser, "dark");
    await page.evaluate(() => {
      window.RichTimeline.applySnapshot(window.__model, {
        entityId: "northwind", threadId: "thr_fem", mode: "ceo", bindingRevision: 1,
        items: [{ id: "turn_two:text:0", entityId: "northwind", threadId: "thr_fem", turnId: "turn_two",
          bindingRevision: 1, createdAt: 1787959900000, sequence: 0, slot: "stream", visibility: "ceo",
          kind: "rich_message", phase: "unknown", text: "On it." }],
      });
      window.__renderOnly();
    });
    const left = await page.evaluate(() => Array.from(document.querySelectorAll(".tl-notice--team")).map((c) => c.dataset.teamKind));
    assertEqual(left, KINDS, "the reload dropped or de-attributed a notice he had already been handed");
    await page.close();
    return `all ${KINDS.length} attributed cards are still on screen after the reload`;
  });

  // ---- 8: contrast ---------------------------------------------------------------------------
  for (const theme of ["dark", "light"]) {
    await run.check(`every kind clears WCAG AA in ${theme} mode, computed`, async () => {
      const page = await everyKind(browser, theme);
      const rows = await page.evaluate((kinds) =>
        kinds.map((kind) => {
          const card = document.querySelector(`.tl-notice[data-team-kind="${kind}"]`);
          const from = card.querySelector(".tl-notice-from");
          const body = card.querySelector(".tl-notice-body");
          const about = card.querySelector(".tl-notice-about");
          const s = getComputedStyle(card);
          return {
            kind,
            from: window.__ratio(from),
            fromPx: parseFloat(getComputedStyle(from).fontSize),
            about: about ? window.__ratio(about) : null,
            body: window.__ratio(body),
            bodyPx: parseFloat(getComputedStyle(body).fontSize),
            rule: window.__ratio(card, "borderLeftColor"),
            ruleWidth: parseFloat(s.borderLeftWidth),
          };
        }), KINDS);
      const worst = { from: Infinity, body: Infinity, rule: Infinity };
      for (const r of rows) {
        assert(r.from >= 4.5, `${theme} ${r.kind}: the from-line is ${r.from.toFixed(2)}:1, under 4.5:1`);
        assert(r.about == null || r.about >= 4.5, `${theme} ${r.kind}: the title is ${r.about && r.about.toFixed(2)}:1`);
        assert(r.fromPx >= 16, `${theme} ${r.kind}: the from-line is ${r.fromPx}px, under the 16px readable floor`);
        assert(r.body >= 4.5, `${theme} ${r.kind}: the body is ${r.body.toFixed(2)}:1, under 4.5:1`);
        assert(r.bodyPx >= 16, `${theme} ${r.kind}: the body is ${r.bodyPx}px`);
        assert(r.rule >= 3, `${theme} ${r.kind}: the left rule is ${r.rule.toFixed(2)}:1, under the 3:1 indicator floor`);
        assert(r.ruleWidth >= 2, `${theme} ${r.kind}: the left rule is ${r.ruleWidth}px`);
        worst.from = Math.min(worst.from, r.from, r.about == null ? Infinity : r.about);
        worst.body = Math.min(worst.body, r.body);
        worst.rule = Math.min(worst.rule, r.rule);
      }
      // THE TWO TONES ARE REALLY TWO: a danger rule that resolved to the attention color would
      // pass every ratio above and carry no signal.
      const tones = new Set(rows.map((r) => (DANGER.has(r.kind) ? "d" : "a") + r.rule.toFixed(3)));
      assertEqual(tones.size, 2, `${theme}: the kinds do not resolve to exactly two rule colors: ${[...tones].join(",")}`);
      const detail = rows.map((r) => `${r.kind} ${r.from.toFixed(2)}/${r.body.toFixed(2)}/${r.rule.toFixed(2)}`).join("; ");
      await page.close();
      return `from-line/body/rule per kind: ${detail}. Worst: from ${worst.from.toFixed(2)}:1, body ${worst.body.toFixed(2)}:1, rule ${worst.rule.toFixed(2)}:1, all at 16px`;
    });
  }

  await browser.close();
  process.exitCode = run.report() ? 1 : 0;
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

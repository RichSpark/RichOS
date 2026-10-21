// THE TECHNICAL VIEW, SWITCHED OFF WHILE A RELOAD THAT ASKED FOR IT IS STILL IN FLIGHT.
//
// Review rv-20261021T195352Z-f24b36ae-2380, on the failure class
// docs/development/failure-classes/machinery-in-the-user-view.md: `confirmTechyScope` bumps
// the view epoch, then awaits the setting write. A turn that ends during that await starts a
// reload that still sees the view ON, so it asks for `get_machinery`. If that reply lands
// after the switch-off reload, the conversation showed machinery rows again with the switch
// off: the user's own view, carrying `control_request` and "Between turns".
//
// No browser and no clock: the SHIPPED functions (`techyOn`, `loadTimeline`,
// `confirmTechyScope`, `renderBetweenTurns`) and the shipped `timeline.js` run in a sandbox,
// and the two bridge replies are released by hand in the order that went wrong. The machinery
// reply is the committed live-types fixture (`fixtures/machinery-payload.json`).
//
// Red before the fix (main.js `loadTimeline` without the `techy !== techyOn()` clause):
// "after the stale reply: technical rows on screen" — model.technical true, 1 technical row,
// the Between turns section shown, with the switch off.
//
// Run: node techy-race.js

"use strict";

const fs = require("fs");
const path = require("path");
const vm = require("vm");
const { createRun, assert, assertEqual, UI_DIR } = require("./lib/harness");

const MAIN = fs.readFileSync(path.join(UI_DIR, "main.js"), "utf8");
const TIMELINE = fs.readFileSync(path.join(UI_DIR, "timeline.js"), "utf8");
const RECORDED = JSON.parse(fs.readFileSync(path.join(__dirname, "fixtures", "machinery-payload.json"), "utf8")).recorded;

/// One top-level function of main.js, exactly as shipped.
function shipped(name) {
  const start = MAIN.search(new RegExp("^(?:async )?function " + name + "\\(", "m"));
  assert(start >= 0, "main.js no longer defines " + name);
  return MAIN.slice(start, MAIN.indexOf("\n}", start) + 2);
}

function deferred() {
  let resolve;
  const promise = new Promise((r) => { resolve = r; });
  return { promise, resolve };
}

function element() {
  return { hidden: true, textContent: "", dataset: {}, children: [],
    appendChild(child) { this.children.push(child); }, setAttribute() {}, removeAttribute() {} };
}

/// A sandbox holding the shipped functions with the view ON, and the hand-released replies.
function sandbox() {
  const calm = Object.assign({}, RECORDED.timeline, { mode: "ceo", betweenTurns: [],
    items: RECORDED.timeline.items.filter((i) => i.visibility === "ceo").map((i) => {
      const copy = Object.assign({}, i); delete copy.detail; return copy;
    }) });
  const write = deferred();
  const read = deferred();
  const c = { console, Map, Set, Promise, window: {}, document: { createElement: element },
    activeThreadId: RECORDED.threadId, threadViewEpoch: 0, threadViews: new Map(),
    techy: { enabled: true }, techyScopeAsk: { on: false }, techyScopeEl: element(),
    TECHY_SCOPE_PRESELECTED: "all-companies", techyScopeInputs: () => [],
    pendingSends: new Map(), conversationEl: { scrollTop: 0 },
    betweenTurnsRowsEl: element(), betweenTurnsQuietEl: element(), betweenTurnsEl: element(),
    renderTechyChip() {}, renderTechySettings() {}, renderTechyState() {},
    rememberThreadView() {}, reviveLiveTurns() {}, scheduleRender() {},
    showUnboundView() {}, threadRow() { return null; },
    invokeQuiet(name) { assertEqual(name, "set_techy_scope"); return write.promise; },
    Bridge: { invoke(name) {
      if (name === "get_machinery") return read.promise;
      assertEqual(name, "get_timeline");
      return Promise.resolve(calm);
    } },
  };
  vm.createContext(c);
  vm.runInContext(TIMELINE, c);
  c.timelineModel = c.window.RichTimeline.createModel();
  c.window.RichTimeline.bind(c.timelineModel, calm.entityId, calm.threadId, 0);
  vm.runInContext(["techyOn", "loadTimeline", "confirmTechyScope", "renderBetweenTurns"].map(shipped).join("\n"), c);
  return { c, write, read };
}

function technicalRows(c) {
  return [...c.timelineModel.items.values()].filter((i) => i.visibility === "technical");
}

async function main() {
  const run = createRun("the technical view stays off when a reload that asked for it answers late");

  await run.check("a reload started while the view is switched off cannot put machinery back", async () => {
    const { c, write, read } = sandbox();
    const toggle = c.confirmTechyScope();   // the CEO turns it off; the write is in flight
    const reload = c.loadTimeline();        // a turn ends meanwhile: this reload sees the view ON
    write.resolve({ enabled: false });
    await toggle;                           // the switch-off reload has drawn the calm view
    assertEqual(c.techyOn(), false);
    assertEqual(c.timelineModel.technical, false, "the switch-off reload did not draw the calm view");
    read.resolve(RECORDED);                 // the stale technical reply lands last
    await reload;
    assertEqual(c.techyOn(), false);
    assertEqual(c.timelineModel.technical, false, "after the stale reply: technical rows on screen");
    assertEqual(technicalRows(c).length, 0, "after the stale reply: technical rows on screen");
    assertEqual(c.betweenTurnsEl.hidden, true, "after the stale reply: the Between turns section is shown");
    return "the late machinery reply was discarded; the calm view stands";
  });

  await run.check("POSITIVE CONTROL: with the view still on, the same reply does draw the machinery", async () => {
    const { c, read } = sandbox();
    const reload = c.loadTimeline();
    read.resolve(RECORDED);
    await reload;
    assertEqual(c.timelineModel.technical, true);
    assert(technicalRows(c).length > 0, "the fixture carries no technical row, so the check above proves nothing");
    return technicalRows(c).length + " technical row(s) drawn while the view is on";
  });

  process.exitCode = run.report() ? 1 : 0;
}

main().catch((e) => {
  console.error(e);
  process.exit(2);
});

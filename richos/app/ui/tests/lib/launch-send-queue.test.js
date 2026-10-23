// Every Send pressed before the launch has opened his conversation reaches Rich, in his order.
//
// The handover review's own fixture (rv-20261020T191206Z-fa2ee4a4-778c,
// `fixtures/startup-send-boundaries.cjs`), kept as the regression test. No browser and no app: it
// runs main.js's real `send()`, `openThread()`, `loadTimeline()` and draft functions over the real
// timeline model, with paint and unrelated backend work stubbed and a controlled activation gate
// in place of wall-clock waits. It records the order `send_message` is issued in.
//
// The defect it guards: after the first Send in that window was held, every later Send returned
// at once. "Please send the proposal today." then "Correction: do not send it until I approve.",
// both pressed before opening, sent only the first; the correction sat in the box and Rich acted
// on the instruction without it. The control case is the earlier finding
// (rv-20261020T181204Z-11d18147-df79): a second Send pressed DURING opening must not overtake.
//
// `LAUNCH_SEND_UI_DIR` points it at another copy of `app/ui` (to prove it red on a tree that
// still has the defect); by default it reads the shell next to it.
"use strict";

const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const assert = require("node:assert/strict");

const root = process.env.LAUNCH_SEND_UI_DIR || path.resolve(__dirname, "../..");
const main = fs.readFileSync(path.join(root, "main.js"), "utf8");
function source(name) {
  const start = main.search(new RegExp("^(?:async )?function " + name + "\\(", "m"));
  assert(start >= 0, name);
  return main.slice(start, main.indexOf("\n}", start) + 2);
}
// The launch-hold state, declared exactly as main.js declares it, whatever it is called.
const launchState = main.match(/^let launch\w+ = [^\n]*;$/gm) || [];
assert(launchState.length >= 2, "main.js no longer declares its launch-hold state; update this test");
// The send line (`sendLine`, `sendWaitingForAcceptance`), declared exactly as main.js declares it.
const sendLineState = main.match(/^let send(?:Line|WaitingForAcceptance) = [^\n]*;$/gm) || [];
assert(sendLineState.length === 2, "main.js no longer declares its send-line state; update this test");

function makeShell() {
  const c = vm.createContext({ console });
  c.window = c;
  vm.runInContext(fs.readFileSync(path.join(root, "timeline.js"), "utf8"), c);
  vm.runInContext(
    launchState.concat(sendLineState).join("\n") +
      `
    const noop = () => {};
    const inputEl = {value: '', disabled: false};
    const conversationEl = {scrollTop: 0};
    const composerBlockedEl = {}, composerScopeEl = {};
    let activeThreadId = null;
    let draftEntityId = null, viewEntityId = null, mainView = 'conversation';
    let sendBlockedReason = null, foldingAttachments = false, followBottom = true;
    let openingThread = null, timelineModel = RichTimeline.createModel();
    let savedWork, workStatusRead = 0, workerCounts, quotaActivity, assignments;
    let firstRunState, firstRunRead = 0, drillItems, techy;
    let navTicket = 0, handNavigations = 0, carriedPendingUserId = null;
    let activeContext = null, threadViewEpoch = 0, pendingScrollRestore = null;
    const drafts = new Map(), scrollTops = new Map(), threadViews = new Map();
    const pendingSends = new Map();
    const ENTITY_DRAFT_PREFIX = 'entity:';
    const issued = [];
    let releaseActivation;
    const activation = new Promise(r => {releaseActivation = r;});
    const Bridge = {invoke: async (name, args) => {
      if (name === 'send_message') {
        if (!args.threadId) throw new Error('null thread');
        issued.push(args.text);
      }
      if (name === 'get_timeline') return {items: []};
      return null;
    }};
    const switchThreadInOrder = () => activation;
    const threadRow = id => ({id, entity_id: 'acme', binding_revision: 1});
    const threadViewKey = (e, t) => JSON.stringify([e, t]);
    const el = () => ({}), isNarrow = () => false, techyOn = () => false;
    const rememberThreadView = noop, clearLiveMark = noop;
    const renderTechyChip = noop, renderTechyState = noop, renderBetweenTurns = noop;
    const renderDrillChip = noop, closeSlideOver = noop, closeThreadMenu = noop;
    const closeWorkerInspector = noop, setRailOpen = noop, renderScopeHeader = noop;
    const flushRender = noop, syncComposerMode = noop, renderRail = noop;
    const startOrStopWaitTimer = noop, announce = noop, autoGrow = noop;
    const scheduleRender = noop, parkViewStateNow = noop, renderWaitBand = noop;
    const refreshActiveContext = noop, refreshTechy = noop, updateJumpButton = noop;
    const resetWaitBandForThread = noop, pollWorkerStatus = noop;
    const renderFirstRunNotice = noop, drainWorkNotices = noop;
    const reviveLiveTurns = noop;
    const setMainView = view => { mainView = view; };
    const showConversationView = () => {mainView = 'conversation'; openingThread = null;};
    const shownPending = () => Array.from(timelineModel.items.values()).filter(i => i.pending).map(i => i.text);
  `,
    c
  );
  for (const name of ["stashThreadViewState", "restoreThreadViewState", "loadTimeline", "send", "openThread"]) {
    vm.runInContext(source(name), c);
  }
  return { run: (code) => vm.runInContext(code, c) };
}

const FIRST = "Please send the proposal today.";
const SECOND = "Correction: do not send it until I approve.";

async function runCase(secondBeforeOpening) {
  const s = makeShell();
  s.run(`inputEl.value = ${JSON.stringify(FIRST)}`);
  await s.run("send()");
  assert.deepEqual(JSON.parse(s.run("JSON.stringify(issued)")), [], "the first Send went out with no thread to send it to");
  assert.equal(s.run("inputEl.value"), "", "the first Send left his words in the box");
  let shownBeforeOpening = null;
  if (secondBeforeOpening) {
    s.run(`inputEl.value = ${JSON.stringify(SECOND)}`);
    await s.run("send()");
    shownBeforeOpening = { box: s.run("inputEl.value"), shown: JSON.parse(s.run("JSON.stringify(shownPending())")) };
  }
  const opening = s.run("openThread('general', {restore: true})");
  if (!secondBeforeOpening) {
    s.run(`inputEl.value = ${JSON.stringify(SECOND)}`);
    await s.run("send()");
  }
  s.run("releaseActivation()");
  await opening;
  s.run("launchLanded = true");
  const after = JSON.parse(s.run("JSON.stringify({sent: issued, composer: inputEl.value, view: mainView, pending: pendingSends.size})"));
  return { ...after, shownBeforeOpening };
}

(async () => {
  const control = await runCase(false);
  console.log("CONTROL (second Send during opening):", JSON.stringify(control));
  assert.deepEqual(control.sent, [FIRST, SECOND], "a Send pressed while his thread opened overtook the held one");
  assert.equal(control.composer, "");

  const early = await runCase(true);
  console.log("SECOND SEND BEFORE OPENING:", JSON.stringify(early));
  assert.deepEqual(early.shownBeforeOpening.box, "", "the second Send before opening left his words in the box");
  assert.deepEqual(early.shownBeforeOpening.shown, [FIRST, SECOND], "the held messages were not both on screen at once, in order");
  assert.deepEqual(early.sent, [FIRST, SECOND], "both messages he sent before his thread opened must reach Rich, in his order");
  assert.equal(early.composer, "", "a sent message is still in the box");
  console.log("PASS  every Send pressed before his thread opened reaches Rich, in his order");
})().catch((e) => {
  console.log("FAIL  " + e.message);
  process.exitCode = 1;
});

// THE BACKGROUND-WORK SURFACE, against the REAL renderer under WebKit.
//
// The background-work spec (richos-hq `docs/plans/background-work-spec-2026-09-17.md`
// revision 5, `9255e71a`), §0 rows 6 and 7, §3.5, §4.2 and §7.8.
//
// TWO THINGS THIS SUITE EXISTS TO PIN, and the first one is a sentence rather than a
// mechanism:
//
//   1. **An unfinished job is never described as finished.** §7.8's rule — *"the wording is
//      the test, not a detail of it"* — survives the CEO's ruling §52 (2026-09-18); its
//      example does not. §7.8's two sentences were "Ready for you to approve" and "done",
//      because the one thing that could stop a job was its land. *"There's nothing that ever
//      not lands on its own here in the terminal … So, yes, always land on its own"*, so the
//      sentence a stopped job actually carries is now "Waiting for your decision before it
//      can go on" — a decision of his on some step, with no claim about his repository,
//      which a job that lands on its own cannot make. This reads the words off the rendered
//      DOM, and the negative scan carries a POSITIVE CONTROL — the same scan run against the
//      `settled` row, which DOES speak of finishing — because a forbidden-word scan that can
//      never fire passes for its own reasons.
//
//   2. **A result waits for the boundary and is never dropped** (§3.5, §0 row 6). The hold
//      is in `main.js`, so that half is read from the source rather than driven: the whole
//      shell needs a live Tauri bridge to run a turn, and a suite that mocked one would be
//      asserting about its own mock. What IS driven here is the renderer.
//
// CONTRAST IS COMPUTED, NEVER EYEBALLED, and in BOTH themes — the standing floor
// (`CLAUDE.md`, "Contrast — WCAG AA, ALWAYS, BOTH THEMES"). The ratios come out of WebKit's
// own resolved colors through `getComputedStyle`, composited where the palette uses alpha,
// so they are the pixels he actually gets rather than the values the stylesheet names.
// Nothing on this surface is declared exempt: an assignment's title, its state and the
// control that stops it are all text he is expected to read.

"use strict";

const fs = require("fs");
const path = require("path");
const { loadPlaywright, createRun, assert, assertEqual, UI_DIR } = require("./lib/harness");

const MAIN_JS = path.join(UI_DIR, "main.js");

/// The page body. No mock bridge: what is rendered below is the same view object `main.js`
/// builds from `get_assignments`, through the same `work-summary.js` the app ships.
///
/// The stylesheet and `work-summary.js` are attached from DISK afterwards —
/// `setContent` leaves the page on `about:blank`, where a `file://` tag never loads, and a
/// suite that silently rendered without the real stylesheet would report contrast for the
/// browser's defaults. `openPage` below proves both arrived before any check runs.
function body(theme) {
  return `<!DOCTYPE html>
<html lang="en" data-theme="${theme}"><head><meta charset="utf-8"></head><body>
  <div id="slideover"><div id="slideover-body"></div></div>
</body></html>`;
}

function helpers() {
  return `
    window.__stopped = [];
    window.__decided = [];
    window.__renderAssignments = (view) => {
      const body = document.getElementById("slideover-body");
      body.innerHTML = "";
      window.RichWorkSummary.renderAssignments(
        view,
        body,
        (id) => window.__stopped.push(id),
        (requestId, allow) => window.__decided.push({requestId, allow})
      );
    };
    /// The rendered ratio, from WebKit's own resolved colors, alpha composited against the
    /// real ancestor background. Never the stylesheet's named value.
    window.__ratio = (selector) => {
      const node = document.querySelector(selector);
      if (!node) throw new Error("no node for " + selector);
      const parse = (value) => {
        const n = value.match(/[\\d.]+/g).map(Number);
        return { r: n[0], g: n[1], b: n[2], a: n.length > 3 ? n[3] : 1 };
      };
      let background = null;
      for (let el = node; el; el = el.parentElement) {
        const c = parse(getComputedStyle(el).backgroundColor);
        if (c.a === 1) { background = c; break; }
      }
      if (!background) background = { r: 255, g: 255, b: 255, a: 1 };
      const fg = parse(getComputedStyle(node).color);
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
    window.__fontPx = (selector) =>
      parseFloat(getComputedStyle(document.querySelector(selector)).fontSize);
  `;
}

/// A page with the SHIPPING stylesheet and the SHIPPING renderer attached from disk, and a
/// check that both really arrived: a missing stylesheet would leave every contrast
/// measurement below describing WebKit's defaults, which is the quietest way for a contrast
/// suite to be wrong.
async function openPage(browser, theme) {
  const page = await browser.newPage({ viewport: { width: 420, height: 900 } });
  await page.setContent(body(theme));
  await page.addStyleTag({ path: path.join(UI_DIR, "style.css") });
  await page.addScriptTag({ path: path.join(UI_DIR, "work-summary.js") });
  await page.addScriptTag({ content: helpers() });
  const wired = await page.evaluate(() => ({
    renderer: typeof window.RichWorkSummary?.renderAssignments === "function",
    // `--card` only exists if the product stylesheet loaded.
    styled: getComputedStyle(document.documentElement).getPropertyValue("--card").trim(),
  }));
  assert(wired.renderer, "the shipping work-summary renderer did not load");
  assert(wired.styled.length > 0, "the shipping stylesheet did not load; contrast here would be the browser's");
  return page;
}

const FORBIDDEN = ["done", "finished", "complete", "landed"];

async function main() {
  const run = createRun("background work: the assignment surface and its wording");
  const browser = await loadPlaywright().webkit.launch();

  await run.check("a blocked assignment says it is waiting on his decision and never done", async () => {
    const page = await openPage(browser, "dark");
    const errors = [];
    page.on("pageerror", (e) => errors.push(String(e)));
    await page.evaluate(() =>
      window.__renderAssignments({ rows: [
        { id: "a1", title: "landing the three branches", state: "blocked", detail: "", repositories: [],
          registeredAtMs: 1, canStop: true, onTheConnection: false },
        { id: "a2", title: "the nightly build", state: "settled", detail: "", repositories: [],
          registeredAtMs: 2, canStop: false, onTheConnection: false },
      ] })
    );
    const rows = await page.locator("#slideover-body section").all();
    assertEqual(rows.length, 2, "both assignments rendered");
    const blocked = (await rows[0].innerText()).toLowerCase();
    assert(blocked.includes("waiting for your decision"), "the blocked row does not say the decision is his: " + blocked);
    for (const word of FORBIDDEN) {
      assert(!blocked.includes(word), `the blocked row said "${word}": ${blocked}`);
    }
    // **AND IT MAKES NO CLAIM ABOUT HIS REPOSITORY** (CEO ruling §52). The row used to
    // promise "Nothing in your repository has been changed yet", which a job that lands on
    // its own cannot promise: it may reach a step like this AFTER landing something, and the
    // promise would be false while his branch had already moved.
    assert(!blocked.includes("repository"), "the blocked row still claims his repository is untouched: " + blocked);
    // POSITIVE CONTROL: the same scan against the settled row DOES fire, so the clean
    // result above is a fact about the blocked row and not about a scan that never matches.
    const settled = (await rows[1].innerText()).toLowerCase();
    assert(
      FORBIDDEN.some((word) => settled.includes(word)),
      "the forbidden-word scan cannot fire at all; the check above proves nothing: " + settled
    );
    assertEqual(errors.length, 0, "renderer errors");
    await page.close();
    return "blocked says the decision is his, claims nothing about his repository, and never speaks of finishing; the scan is proven able to fail";
  });

  // esc-20260927T093052Z-85f3303f: a task the back end carried out itself, with no helper,
  // closes on its report — the command's result, or "the Acme repository isn't connected".
  // The host never witnessed that as work finishing, so the row must not say "Finished."
  await run.check("a task the back end answered itself says it is in his conversation, never finished", async () => {
    const page = await openPage(browser, "light");
    const errors = [];
    page.on("pageerror", (e) => errors.push(String(e)));
    await page.evaluate(() =>
      window.__renderAssignments({ rows: [
        { id: "t1", title: "count the commits in Acme", kind: "task", state: "settled", detail: "Answered.",
          answered: true, repositories: [], registeredAtMs: 1, canStop: false, onTheConnection: false },
        { id: "t2", title: "the nightly build", kind: "task", state: "settled", detail: "It landed on cc/echo-1 in project.",
          answered: false, repositories: [], registeredAtMs: 2, canStop: false, onTheConnection: false },
      ] })
    );
    const rows = await page.locator("#slideover-body section").all();
    assertEqual(rows.length, 2, "both assignments rendered");
    const answered = (await rows[0].innerText()).toLowerCase();
    assert(/it's in your conversation/i.test(answered), "the answered task does not point at his conversation: " + answered);
    for (const word of FORBIDDEN) {
      assert(!answered.includes(word), `the answered task said "${word}": ${answered}`);
    }
    // The record's internal note never reaches him beside a sentence written for him.
    assert(!answered.includes("answered."), "the record's internal detail was shown: " + answered);
    // POSITIVE CONTROL: a task whose work finished still says so. The branch it landed on is
    // the back end's line, shown only in Technical view (hunt 57, finding 11; `work-summary.js`
    // in this directory proves it comes back there).
    const finished = (await rows[1].innerText()).toLowerCase();
    assert(finished.includes("finished.") && !finished.includes("cc/echo-1"), "a finished task lost its sentence or names its branch: " + finished);
    assertEqual(errors.length, 0, "renderer errors");
    await page.close();
    return "an answered task reads as answered in his conversation; a finished task still reads as finished";
  });

  await run.check("the stop control is per assignment, names it, and only where it can act", async () => {
    const page = await openPage(browser, "dark");
    await page.evaluate(() =>
      window.__renderAssignments({ rows: [
        { id: "a1", title: "landing the three branches", state: "running", detail: "", repositories: [],
          registeredAtMs: 1, canStop: true, onTheConnection: true },
        { id: "a2", title: "the nightly build", state: "running", detail: "", repositories: [],
          registeredAtMs: 2, canStop: true, onTheConnection: false },
        { id: "a3", title: "the old audit", state: "interrupted", detail: "", repositories: [],
          registeredAtMs: 3, canStop: false, onTheConnection: false },
      ] })
    );
    const stops = page.locator("#slideover-body button.assignment-stop");
    assertEqual(await stops.count(), 2, "a stop appeared for an assignment that has already stopped");
    // Spec §4.2: the control belongs where the work is visible, ONE PER ASSIGNMENT. A
    // control labeled only "Stop" beside three of them is one he cannot use without
    // counting rows, so each names its own.
    assertEqual(
      await stops.nth(0).getAttribute("aria-label"),
      "Stop landing the three branches",
      "the first stop does not say which assignment it stops"
    );
    assertEqual(await stops.nth(1).getAttribute("aria-label"), "Stop the nightly build", "second stop label");
    await stops.nth(1).click();
    assertEqual(
      JSON.stringify(await page.evaluate(() => window.__stopped)),
      JSON.stringify(["a2"]),
      "pressing one assignment's stop reached a different assignment"
    );
    await page.close();
    return "one control per open assignment, each naming its own, and the press carries that id";
  });

  for (const theme of ["dark", "light"]) {
    await run.check(`every word on the assignment surface clears WCAG AA in ${theme} mode`, async () => {
      const page = await openPage(browser, theme);
      await page.evaluate(() =>
        window.__renderAssignments({ rows: [
          { id: "a1", title: "landing the three branches", state: "blocked",
            detail: "The work has run and stopped at a step that is yours to decide: running a command on your Mac.",
            repositories: [], registeredAtMs: 1, canStop: true, onTheConnection: false,
            awaitingYou: { requestId: "req-1", asked: "running a command on your Mac",
              tool: "Bash", description: "", raisedAtMs: 2 } },
        ] })
      );
      const measured = [];
      for (const [what, selector, floor] of [
        ["heading", "#slideover-body .assignment-heading", 4.5],
        ["title", "#slideover-body .assignment-title", 4.5],
        ["state and detail", "#slideover-body .overlay-note", 4.5],
        ["stop control", "#slideover-body .assignment-stop", 4.5],
        ["the question", "#slideover-body .assignment-asked", 4.5],
        ["approve control", "#slideover-body .assignment-approve", 4.5],
        ["decline control", "#slideover-body .assignment-decline", 4.5],
      ]) {
        const ratio = await page.evaluate((s) => window.__ratio(s), selector);
        const px = await page.evaluate((s) => window.__fontPx(s), selector);
        measured.push(`${what} ${ratio.toFixed(2)}:1 at ${px}px`);
        assert(ratio >= floor, `${theme}: ${what} is ${ratio.toFixed(2)}:1, under the ${floor}:1 floor`);
        // `ceo-decisions.md` §15: 16px is the floor for text meant to be easily read, and
        // 14px is the skippable tier. None of this is skippable, so none of it is 14px.
        assert(px >= 16, `${theme}: ${what} is ${px}px, under the 16px readable floor`);
      }
      await page.close();
      return measured.join("; ");
    });
  }

  await run.check("the sentence that says it is his decision carries the controls that make it", async () => {
    // **§7.8's whole point, and the gap slice 1 recorded.** The surface said "ready for you
    // to approve" and had no approve control on it, because the desk that holds his decision
    // did not exist. It does now, and the test is that the two arrive together.
    const page = await openPage(browser, "dark");
    const errors = [];
    page.on("pageerror", (e) => errors.push(String(e)));
    await page.evaluate(() =>
      window.__renderAssignments({ rows: [
        { id: "a1", title: "landing the three branches", state: "blocked", detail: "", repositories: [],
          registeredAtMs: 1, canStop: true, onTheConnection: false,
          awaitingYou: { requestId: "req-1", asked: "running a command on your Mac",
            tool: "Bash", description: "", raisedAtMs: 2 } },
        // POSITIVE CONTROL, in the same render: the SAME state with no question on the desk
        // — an assignment that stopped at his decision before a relaunch — has no decision
        // controls and says so, which is what makes the row above a fact about `awaitingYou`
        // rather than about every blocked row growing a pair of buttons.
        { id: "a2", title: "the nightly build", state: "blocked", detail: "", repositories: [],
          registeredAtMs: 2, canStop: true, onTheConnection: false },
      ] })
    );
    const rows = await page.locator("#slideover-body section").all();
    const waiting = await rows[0].innerText();
    assert(waiting.toLowerCase().includes("waiting for your decision"), waiting);
    assert(
      waiting.includes("Waiting on you: running a command on your Mac."),
      "the row does not say what it is waiting on: " + waiting
    );
    assert(!waiting.includes("mcp__"), "the row shows a wire tool name: " + waiting);
    assert(!waiting.includes("Ask Rich"), "the row still points at the composer: " + waiting);
    assertEqual(await rows[0].locator("button.assignment-approve").count(), 1, "no approve control");
    assertEqual(await rows[0].locator("button.assignment-decline").count(), 1, "no decline control");
    assertEqual(
      await rows[0].locator("button.assignment-approve").getAttribute("aria-label"),
      "Approve landing the three branches",
      "the approve control does not say which assignment it approves"
    );
    const without = await rows[1].innerText();
    assert(without.includes("Ask Rich to continue it when you are ready."), without);
    assert(!without.includes("Waiting on you:"), "a row with no request named a step: " + without);
    assertEqual(await rows[1].locator("button.assignment-approve").count(), 0, "a control appeared with no request behind it");
    assertEqual(errors.length, 0, "renderer errors");
    await page.close();
    return "the mandated sentence, the step named in his words, and Approve/Decline — with the no-request row proving the controls are not unconditional";
  });

  await run.check("the press carries the exact request and the exact answer", async () => {
    // §5.2's *"one exact action"* has to survive a queue: what he presses is one REQUEST,
    // not one assignment, and both answers have to reach the backend as themselves.
    const page = await openPage(browser, "dark");
    await page.evaluate(() =>
      window.__renderAssignments({ rows: [
        { id: "a1", title: "landing the three branches", state: "blocked", detail: "", repositories: [],
          registeredAtMs: 1, canStop: true, onTheConnection: false,
          awaitingYou: { requestId: "req-one", asked: "writing a file outside its workspace",
            tool: "Write", description: "", raisedAtMs: 2 } },
        { id: "a2", title: "the nightly build", state: "blocked", detail: "", repositories: [],
          registeredAtMs: 2, canStop: true, onTheConnection: false,
          awaitingYou: { requestId: "req-two", asked: "running a command on your Mac",
            tool: "Bash", description: "", raisedAtMs: 3 } },
      ] })
    );
    await page.locator("#slideover-body section").nth(1).locator("button.assignment-approve").click();
    await page.locator("#slideover-body section").nth(0).locator("button.assignment-decline").click();
    assertEqual(
      JSON.stringify(await page.evaluate(() => window.__decided)),
      JSON.stringify([{requestId: "req-two", allow: true}, {requestId: "req-one", allow: false}]),
      "a press reached the wrong request or the wrong answer"
    );
    await page.close();
    return "approve on the second assignment and decline on the first, each carrying its own request id";
  });

  await run.check("live and durable notice delivery is single and stays on its origin", async () => {
    const vm = require("node:vm");
    const src = fs.readFileSync(MAIN_JS, "utf8");
    const events = {};
    let pending = [], delayed = null;
    const context = vm.createContext({window: {}, console, Date, Map, Promise,
      activeThreadId: "a", followBottom: false, scheduleRender() {}, busy: false,
      Bridge: {listen(name, cb) { events[name] = cb; }, async invoke(name, args) {
        assertEqual(name, "take_work_notices", "only durable notice consumption");
        const result = pending; pending = [];
        if (delayed) await delayed;
        return result;
      }}});
    vm.runInContext(fs.readFileSync(path.join(UI_DIR, "timeline.js"), "utf8"), context);
    vm.runInContext(`
      let timelineModel = window.RichTimeline.createModel();
      window.RichTimeline.bind(timelineModel, "company", "a", 1);
      const origin = timelineModel;
      function anyLiveTurn() { return busy; }
    `, context);
    vm.runInContext(src.slice(src.indexOf("let voiceBusy = false;"), src.indexOf("/// A line Rich says LOCALLY")), context);
    const eventStart = src.indexOf('Bridge.listen("rich://work-notice"');
    vm.runInContext(src.slice(eventStart, src.indexOf("\n});", eventStart) + 4), context);
    const notice = {assignmentId: "one", kind: "failed", text: "Nothing was landed.", raisedAtMs: 100};
    pending = [notice];
    events["rich://work-notice"]({payload: {threadId: "a", notice}});
    await vm.runInContext("drainWorkNotices()", context);
    assertEqual(vm.runInContext("origin.items.size", context), 1, "push followed by saved drain must not duplicate");
    // A real second notice with identical words must survive, not be text-deduplicated.
    pending = [{...notice, assignmentId: "two", raisedAtMs: 101}];
    await vm.runInContext("drainWorkNotices()", context);
    assertEqual(vm.runInContext("origin.items.size", context), 2, "another assignment is another notice");
    // Hold a response over navigation, then hold its delivery over a working turn.
    let release;
    delayed = new Promise(resolve => { release = resolve; });
    pending = [{...notice, assignmentId: "three", raisedAtMs: 102}];
    const heldDrain = vm.runInContext("busy = true; drainWorkNotices()", context);
    await Promise.resolve();
    vm.runInContext(`activeThreadId = "b"; timelineModel = window.RichTimeline.createModel();
      window.RichTimeline.bind(timelineModel, "company", "b", 1);`, context);
    release(); delayed = null;
    await heldDrain;
    vm.runInContext("busy = false; flushWorkNotices()", context);
    assertEqual(vm.runInContext("timelineModel.items.size", context), 0, "held notice must not leak into destination");
    assertEqual(vm.runInContext("origin.items.size", context), 3, "origin retains delayed notice");
    return "one durable consumer; equal text from another assignment survives; delayed notice keeps origin";
  });

  await run.check("in voice mode a held card appears once he is listening, never while either one speaks", async () => {
    // DRIVEN through the real `renderVoiceState`, because the defect was in which states it
    // called calm: it waited for "idle", a state the voice engine never reports (its states
    // are off, listening, hearing, thinking and speaking — `richos-voice` `VoiceState::as_str`),
    // so with voice mode on every held card stayed hidden until voice mode was turned off.
    const vm = require("node:vm");
    const src = fs.readFileSync(MAIN_JS, "utf8");
    const row = () => ({hidden: false});
    const context = vm.createContext({window: {}, console, Date, Map, Promise,
      followBottom: false, scheduleRender() {},
      voiceListeningEl: row(), voiceNoAudioEl: row(), voiceSpeakingEl: row()});
    vm.runInContext(fs.readFileSync(path.join(UI_DIR, "timeline.js"), "utf8"), context);
    vm.runInContext(`
      let timelineModel = window.RichTimeline.createModel();
      window.RichTimeline.bind(timelineModel, "company", "a", 1);
      let voiceMode = true;
      function anyLiveTurn() { return false; }
    `, context);
    vm.runInContext(src.slice(src.indexOf("let voiceBusy = false;"), src.indexOf("/// A line Rich says LOCALLY")), context);
    const renderStart = src.indexOf("function renderVoiceState(");
    vm.runInContext(src.slice(renderStart, src.indexOf("\n}\n", renderStart) + 3), context);
    const shown = () => vm.runInContext("timelineModel.items.size", context);
    vm.runInContext('renderVoiceState("speaking", false)', context);
    vm.runInContext('receiveWorkNotice("Waiting until you sign in.", 100)', context);
    assertEqual(shown(), 0, "a card appeared while Rich was speaking");
    vm.runInContext('renderVoiceState("hearing", false)', context);
    assertEqual(shown(), 0, "a card appeared while the CEO was talking");
    vm.runInContext('renderVoiceState("thinking", false)', context);
    assertEqual(shown(), 0, "a card appeared mid-exchange, before Rich answered");
    vm.runInContext('renderVoiceState("listening", false)', context);
    assertEqual(shown(), 1, "the held card did not appear once voice mode was resting in listening");
    // Arriving while he is already listening, it shows at once rather than being held.
    vm.runInContext('receiveWorkNotice("Waiting for your decision before it can go on.", 101)', context);
    assertEqual(shown(), 2, "a card arriving while listening was held instead of shown");
    return "held through speaking, hearing and thinking; shown on listening; shown at once while listening";
  });

  await run.check("in voice mode a job's result written into the conversation is SAID, once, never inside a turn", async () => {
    // Nightly 57's hunt, findings 4 and 6, and the handover review rv-20261015T122526Z-558b93b7:
    // "Will is on your team now" is written into the conversation as Rich's message, which the
    // window draws from `rich://proactive-message` and speaks nothing. Driven here through the
    // shipping listener and the shipping background-work section, with the bridge recorded.
    const vm = require("node:vm");
    const src = fs.readFileSync(MAIN_JS, "utf8");
    const events = {};
    const calls = [];
    const context = vm.createContext({window: {}, console, Date, Map, Promise,
      activeThreadId: "hiring", followBottom: false, scheduleRender() {}, busy: false, voiceMode: true,
      Bridge: {listen(name, cb) { events[name] = cb; }, invoke(name, args) {
        calls.push(args && args.text !== undefined ? `${name}: ${args.text}` : name);
        return Promise.resolve();
      }}});
    vm.runInContext(fs.readFileSync(path.join(UI_DIR, "timeline.js"), "utf8"), context);
    vm.runInContext(`
      let timelineModel = window.RichTimeline.createModel();
      function anyLiveTurn() { return busy; }
    `, context);
    vm.runInContext(src.slice(src.indexOf("let voiceBusy = false;"), src.indexOf("/// A line Rich says LOCALLY")), context);
    const eventStart = src.indexOf('Bridge.listen("rich://work-result"');
    assert(eventStart >= 0, "nothing listens for rich://work-result: a result written into the conversation is never said");
    vm.runInContext(src.slice(eventStart, src.indexOf("\n});", eventStart) + 4), context);
    const settle = () => new Promise((resolve) => setTimeout(resolve, 0));
    const joined = "Will (Marketing Director) is on your team now, ready to take work.";
    const beth = "Beth (Bookkeeper) is on your team now, ready to take work.";

    // The review's case: voice mode on, the microphone listening (the panel's resting state,
    // which `renderVoiceState` records as voiceBusy), no turn running.
    vm.runInContext("voiceBusy = true", context);
    events["rich://work-result"]({payload: {threadId: "hiring", text: joined}});
    await settle();
    assertEqual(calls, [`voice_speak_result: ${joined}`], "the result was not said");

    // Inside a turn it waits for the boundary and is then said once, in order.
    calls.length = 0;
    vm.runInContext("busy = true", context);
    events["rich://work-result"]({payload: {threadId: "hiring", text: beth}});
    await settle();
    assertEqual(calls, [], "a result was said inside a turn");
    vm.runInContext("busy = false; flushWorkNotices(); flushWorkNotices()", context);
    await settle();
    assertEqual(calls, [`voice_speak_result: ${beth}`], "the held result was not said once at the boundary");

    // Not in voice mode, or about another conversation: nothing is said.
    calls.length = 0;
    vm.runInContext("voiceMode = false", context);
    events["rich://work-result"]({payload: {threadId: "hiring", text: joined}});
    vm.runInContext("voiceMode = true", context);
    events["rich://work-result"]({payload: {threadId: "elsewhere", text: joined}});
    await settle();
    assertEqual(calls, [], "a result was said with voice mode off, or from another conversation");

    // The shell emits it, with its words, for every result the spine writes, on both paths.
    const shell = fs.readFileSync(path.join(UI_DIR, "..", "src-tauri", "src", "main.rs"), "utf8");
    assert(shell.includes('pub const EVENT_WORK_RESULT: &str = "rich://work-result";'), "the shell declares no rich://work-result");
    const writer = shell.slice(shell.indexOf("fn say_work_results("), shell.indexOf("fn tell_work_results("));
    assertEqual((writer.match(/tell_work_results\(/g) || []).length, 2, "a path that writes results does not tell the window");
    assert(/fn tell_work_results[\s\S]{0,400}?emit\(EVENT_WORK_RESULT, serde_json::json!\(\{"threadId": thread_id, "text": text\}\)\)/.test(shell),
      "the shell does not emit each written result's words");
    // ...and its speaker command says them as words of their own, which a Stop he gave an
    // earlier reply does not silence (review rv-20261015T125238Z-37f970a3; the gate's rule is
    // `richos-voice` turn.rs a_result_after_he_stopped_rich_is_still_said_and_never_over_him).
    assert(/fn voice_speak_result\(app: AppHandle, text: String\)[\s\S]{0,300}?ctl\.speak_result\(&text\)/.test(shell),
      "voice_speak_result does not reach the controller's speak_result");
    assert(/\n\s+voice_speak_result\n\s+\]\)/.test(shell), "voice_speak_result is not registered");
    return "said in voice mode while listening; held through a turn and said once after; silent with voice off or elsewhere";
  });

  await run.check("a background result is HELD while a turn runs, and never dropped", async () => {
    // Read from the source, not driven: the hold lives in the shell, and running a real
    // turn needs a live Tauri bridge. A suite that mocked one would be asserting about the
    // mock. What is asserted here is that the three parts of §3.5's rule exist and are
    // wired to each other, by name.
    const src = fs.readFileSync(MAIN_JS, "utf8");
    assert(src.includes("heldWorkNotices"), "there is no hold at all");
    assert(
      /function receiveWorkNotice[\s\S]{0,400}?calmEnoughForANotice\(\)[\s\S]{0,200}?heldWorkNotices\.push/.test(src),
      "an arriving notice is not held when the conversation is busy"
    );
    assert(
      /function calmEnoughForANotice[\s\S]{0,200}?!anyLiveTurn\(\)[\s\S]{0,60}?!voiceBusy/.test(src),
      "the hold does not consider both a live turn and a live voice exchange"
    );
    // The flush is on the turn boundary AND on voice going quiet — two boundaries, because
    // there are two ways to be mid-sentence.
    assert(
      /rich:\/\/turn-completed[\s\S]{0,400}?flushWorkNotices\(\)/.test(src),
      "nothing flushes the hold when a turn ends"
    );
    assert(
      /voiceBusy = [\s\S]{0,200}?if \(!voiceBusy\) flushWorkNotices\(\)/.test(src),
      "nothing flushes the hold when the voice exchange ends"
    );
    // POSITIVE CONTROL for the scans above: the same regular expression shape run against a
    // source that does NOT wire them must fail, so a green result is not a pattern that
    // matches anything.
    const gutted = src.replace(/heldWorkNotices\.push/g, "noop");
    assert(
      !/function receiveWorkNotice[\s\S]{0,400}?calmEnoughForANotice\(\)[\s\S]{0,200}?heldWorkNotices\.push/.test(gutted),
      "the hold scan matches a source with the hold removed; it proves nothing"
    );
    // §3.4: the durable read exists and is called where he can see the result — on opening
    // the conversation, and again at the turn boundary.
    assert(src.includes("take_work_notices"), "nothing reads the durable notices");
    // CALL SITES, not occurrences: the declaration contains the same text, and counting it
    // would have made a build that never calls the function pass with one call site.
    assertEqual(
      (src.match(/^\s*drainWorkNotices\(\);/gm) || []).length,
      3,
      "the durable read must run on thread open, turn boundary and live wakeup"
    );
    return "held on a live turn or a live voice exchange, flushed at both boundaries, and read durably on open, boundary and push";
  });

  await browser.close();
  process.exit(run.report() ? 1 : 0);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

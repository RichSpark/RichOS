"use strict";
const path = require("path");
const {loadPlaywright, createRun, assert, assertEqual, UI_DIR, leaveHome, openThread} = require("./lib/harness");

/// Wait for a line to reach the chip, then RETURN what the chip says — and on failure name
/// what was actually on screen. Playwright's own `TimeoutError: page.waitForFunction: Timeout
/// 8000ms exceeded` says nothing about the defect this is here to catch, and the difference
/// between "the chip was empty" and "the chip said something else" is the whole diagnosis.
async function chipWithin(page, needle, where) {
  try {
    await page.waitForFunction(
      (n) => ((document.getElementById("drill-chip-zone") || {}).textContent || "").includes(n),
      needle,
      {timeout: 8000}
    );
  } catch (_e) {
    const text = await page.textContent("#drill-chip-zone");
    assert(
      false,
      where + ": the chip never carried " + JSON.stringify(needle) + " with the 3,000 ms refill " +
        "blocked — it reads " + JSON.stringify(((text || "")).trim()) + ". With that interval gone, " +
        "openThread's leading pollWorkerStatus() is the only thing that can fill it."
    );
  }
  return page.textContent("#drill-chip-zone");
}

async function main() {
  const run = createRun("saved work in the shipping renderer");
  const browser = await loadPlaywright().webkit.launch();
  await run.check("saved outcomes remain distinct and scoped when the conversation changes", async () => {
    const page = await browser.newPage({viewport: {width:1280, height:900}});
    const errors = []; page.on("pageerror", e => errors.push(String(e)));
    await page.addInitScript(() => window.__RICHOS_MOCK_PRESET__ = {workSummaries: {
      hiring: {items:[{title:"<img src=x onerror=alert(1)>", role:"worker", repository:"/fictional/one", state:"run-ended", detail:"Worker run ended. Review is still required."}, {title:"Reviewed change", role:"worker", repository:"/fictional/two", state:"integrated", detail:"Reviewed commit integrated locally. Whole assignment remains open."}], omitted:2}
    }});
    await page.goto("file://" + path.join(UI_DIR,"index.html")); await leaveHome(page);
    await openThread(page,"hiring");
    await page.waitForFunction(() => document.getElementById("drill-chip-zone").textContent.includes("2 team jobs"));
    await page.click(".drill-chip");
    assert((await page.textContent("#slideover-body")).includes("Review is still required"), "run end became success");
    assert((await page.textContent("#slideover-body")).includes("Whole assignment remains open"), "integration became whole-task completion");
    assertEqual(await page.locator("#slideover-body img").count(),0,"receipt rendered as markup");
    await page.click("#slideover-close"); await openThread(page,"acme");
    assert(!(await page.textContent("#drill-chip-zone")).includes("2 team jobs"),"old-company receipt leaked");
    // A read that fails keeps this conversation's last reading, and says why only in Technical
    // view (hunt 57, finding 12).
    await openThread(page,"hiring");
    await page.waitForFunction(() => document.getElementById("drill-chip-zone").textContent.includes("2 team jobs"));
    await page.evaluate(async () => { window.__RICHOS_MOCK_PRESET__.workSummaryError = "Synthetic damaged receipt"; await pollWorkerStatus(); });
    const kept = await page.textContent("#drill-chip-zone");
    assert(kept.includes("2 team jobs") && !/unavailable/i.test(kept), "a failed read replaced the last reading: " + JSON.stringify(kept.trim()));
    await page.click(".drill-chip");
    assert(!(await page.textContent("#slideover-body")).includes("Synthetic damaged receipt"), "the failed read's reason is shown outside Technical view");
    await page.evaluate(async () => {
      await window.RichBridge.invoke("set_techy_default", {enabled: true});
      await refreshTechy(activeThreadId);
      renderSlideOver();
    });
    assert((await page.textContent("#slideover-body")).includes("Synthetic damaged receipt"),"with Technical view on, the failed read is not explained");
    assertEqual(errors.length,0,"renderer errors"); await page.close();
    return "scoped receipts, explicit outcomes, escaped content; a failed read keeps the last reading and says why in Technical view";
  });
  // **HUNT 57, FINDING 12 (2026-10-15): A BRAND-NEW CONVERSATION SAYS NOTHING ABOUT SAVED WORK.**
  // Nightly 57 showed "Saved work unavailable" under the very first conversation of 2 fresh
  // installs: `get_work_status` refuses while Rich's session is being started ("Work status is
  // changing. Please try again.", `main.rs` `get_work_status`), and the status line printed that
  // refusal as if something were wrong. A read that fails now keeps what this conversation last
  // read, which for a new one is nothing, so nothing about saved work is shown.
  await run.check("a brand-new conversation whose saved-work read is refused while Rich gets ready says nothing about saved work", async () => {
    const page = await browser.newPage({viewport: {width:1280, height:900}});
    const errors = []; page.on("pageerror", e => errors.push(String(e)));
    await page.addInitScript(() => window.__RICHOS_MOCK_PRESET__ = {workSummaryError: "Work status is changing. Please try again."});
    await page.goto("file://" + path.join(UI_DIR,"index.html")); await leaveHome(page);
    await openThread(page,"hiring");
    // The read is made and refused here, rather than left to the 3,000 ms refill.
    const read = await page.evaluate(async () => { await pollWorkerStatus(); return savedWork.error || null; });
    assert(read && read.includes("Work status is changing"), "the refused read never happened, so this check proves nothing: " + JSON.stringify(read));
    const chip = (await page.textContent("#drill-chip-zone")) || "";
    assert(!/unavailable/i.test(chip), "a brand-new conversation's status line reads " + JSON.stringify(chip.trim()));
    assert(!/team job|saved work/i.test(chip), "the status line speaks of saved work with nothing saved: " + JSON.stringify(chip.trim()));
    assertEqual(errors.length,0,"renderer errors"); await page.close();
    return "refused read (" + JSON.stringify(read) + "); the status line reads " + JSON.stringify(chip.trim());
  });
  // **HUNT 57, FINDING 11 (2026-10-15): THE WORK PANELS SPEAK TO A BUSINESS OWNER.** Nightly 57's
  // hiring job showed "consult · no repository" under every row of Under the hood and "4 saved
  // work records" above the message box. The status line now counts team jobs, and the machinery
  // line (the helper's role and its repository) shows only with Technical view on, where it
  // still reads exactly as before.
  await run.check("the status line and Under the hood say nothing about records, consults or repositories unless Technical view is on", async () => {
    const page = await browser.newPage({viewport: {width:1280, height:900}});
    const errors = []; page.on("pageerror", e => errors.push(String(e)));
    await page.addInitScript(() => window.__RICHOS_MOCK_PRESET__ = {workSummaries: {
      hiring: {items:[{title:"Research brief for new teammate Beth, Bookkeeper", role:"consult", repository:"no repository", state:"answered", detail:"Done. The teammate's answer went back to Rich."}], omitted:3}
    }});
    await page.goto("file://" + path.join(UI_DIR,"index.html")); await leaveHome(page);
    await openThread(page,"hiring");
    const chip = await chipWithin(page, "team job", "on hiring");
    assert(chip.includes("1 team job"), "the status line reads " + JSON.stringify(chip.trim()));
    assert(!/record/i.test(chip), "the status line still counts records: " + JSON.stringify(chip.trim()));
    await page.click(".drill-chip");
    const plain = await page.textContent("#slideover-body");
    for (const word of ["repositor", "consult", "record"]) {
      assert(!plain.toLowerCase().includes(word), "Under the hood names " + JSON.stringify(word) + ": " + JSON.stringify(plain.trim()));
    }
    assert(plain.includes("Done. The teammate's answer went back to Rich."), "the row's own sentence is missing: " + JSON.stringify(plain.trim()));
    assert(plain.includes("3 older team jobs aren't shown here."), "the older jobs are not counted in plain words: " + JSON.stringify(plain.trim()));
    // POSITIVE CONTROL: with Technical view on, the machinery line is still there, as it was.
    await page.evaluate(async () => {
      await window.RichBridge.invoke("set_techy_default", {enabled: true});
      await refreshTechy(activeThreadId);
      renderSlideOver();
    });
    const technical = await page.textContent("#slideover-body");
    assert(technical.includes("consult · no repository"), "Technical view lost the machinery line: " + JSON.stringify(technical.trim()));
    assertEqual(errors.length,0,"renderer errors"); await page.close();
    return "plain: " + JSON.stringify(chip.trim()) + ", no machinery line; Technical view: the line is back";
  });
  // **THE ASSIGNMENT ROWS IN THE SAME PANEL, UNDER THE SAME RULE** (hunt 57, finding 11; the
  // review of 43ab0a0c8). "What you have asked for" said "Getting a workspace ready." and,
  // after a state's sentence, whatever the back end recorded ("The back end couldn't take it
  // yet", "Opening the work connection.", "It landed on cc/echo-1 in project."). In the
  // ordinary view each row is its title and the state's plain sentence; the recorded detail
  // shows only with Technical view on. Job details are supplied by Stu, so these
  // fixtures exercise arbitrary report text rather than app-written progress sentences.
  await run.check("the assignment rows in Under the hood speak plain words, and the back end's own lines show only in Technical view", async () => {
    const retry = "The reviewer has not replied yet.";
    const page = await browser.newPage({viewport: {width:1280, height:900}});
    const errors = []; page.on("pageerror", e => errors.push(String(e)));
    await page.addInitScript((retry) => window.__RICHOS_MOCK_PRESET__ = {assignments: {hiring: [
      {id: "p", title: "Prepare the report", state: "preparing", detail: "Opening the work connection.",
       repositories: [], registeredAtMs: 1, canStop: true, onTheConnection: false, awaitingYou: null},
      {id: "b", title: "Hire a bookkeeper", state: "blocked", detail: retry,
       repositories: [], registeredAtMs: 2, canStop: true, onTheConnection: false, awaitingYou: null},
      {id: "s", title: "The nightly build", state: "settled", detail: "It landed on cc/echo-1 in project.",
       repositories: [], registeredAtMs: 3, canStop: false, onTheConnection: false, awaitingYou: null},
    ]}}, retry);
    await page.goto("file://" + path.join(UI_DIR,"index.html")); await leaveHome(page);
    await openThread(page,"hiring");
    await chipWithin(page, "assignment", "on hiring");
    await page.click(".drill-chip");
    await page.waitForFunction(() => document.querySelectorAll("#slideover-body .assignment-title").length === 3);
    const plain = await page.textContent("#slideover-body");
    for (const words of ["workspace", "back end", "connection", "cc/echo-1", "landed"]) {
      assert(!plain.toLowerCase().includes(words), "What you have asked for says " + JSON.stringify(words) + ": " + JSON.stringify(plain.trim()));
    }
    // The rows are still there, each with its own sentence.
    for (const sentence of ["Getting ready to start.", "Waiting for your decision.", "Finished."]) {
      assert(plain.includes(sentence), "a row lost its sentence " + JSON.stringify(sentence) + ": " + JSON.stringify(plain.trim()));
    }
    // POSITIVE CONTROL: with Technical view on, every recorded line is back.
    await page.evaluate(async () => {
      await window.RichBridge.invoke("set_techy_default", {enabled: true});
      await refreshTechy(activeThreadId);
      renderSlideOver();
    });
    const technical = await page.textContent("#slideover-body");
    for (const line of ["Opening the work connection.", retry, "It landed on cc/echo-1 in project."]) {
      assert(technical.includes(line), "Technical view lost " + JSON.stringify(line) + ": " + JSON.stringify(technical.trim()));
    }
    assertEqual(errors.length,0,"renderer errors"); await page.close();
    return "plain: three rows, three plain sentences, no back-end lines; Technical view: all three lines are back";
  });
  // **BUT A LINE THAT TELLS HIM WHAT TO DO STAYS IN THE ORDINARY VIEW** (the review of
  // 25b540804, rv-20261015T113335Z-25b54080-3123). Hiding every recorded line also hid
  // "Waiting for you to sign in to Claude, in Settings under Claude accounts." under a held job,
  // which then read only "Written down. Nothing has been prepared yet.", and the long-wait line
  // that names Stop. A Stu-authored waiting sentence marked `forYou` remains visible
  // in both views. There is no list of app-generated job status sentences.
  await run.check("a line that tells him what to do shows in the ordinary view, and the back end's own lines still do not", async () => {
    const signIn = "Please sign in to Claude in Settings.", longer = "You can stop this job while its reviewer is working.";
    const page = await browser.newPage({viewport: {width:1280, height:900}});
    const errors = []; page.on("pageerror", e => errors.push(String(e)));
    await page.addInitScript(({signIn, longer}) => window.__RICHOS_MOCK_PRESET__ = {assignments: {hiring: [
      {id: "h", title: "Prepare the report", state: "registered", detail: signIn, forYou: true,
       repositories: [], registeredAtMs: 1, canStop: true, onTheConnection: false, held: true, awaitingYou: null},
      {id: "q", title: "How many invoices are open?", kind: "check", state: "registered", detail: signIn, forYou: true,
       repositories: [], registeredAtMs: 2, canStop: true, onTheConnection: false, held: true, awaitingYou: null},
      {id: "r", title: "Hire a bookkeeper", state: "running", detail: longer, forYou: true,
       repositories: [], registeredAtMs: 3, canStop: true, onTheConnection: false, awaitingYou: null},
      {id: "p", title: "The nightly build", state: "preparing", detail: "Opening the work connection.", forYou: false,
       repositories: [], registeredAtMs: 4, canStop: true, onTheConnection: false, awaitingYou: null},
    ]}}, {signIn, longer});
    await page.goto("file://" + path.join(UI_DIR,"index.html")); await leaveHome(page);
    await openThread(page,"hiring");
    await chipWithin(page, "assignment", "on hiring");
    await page.click(".drill-chip");
    await page.waitForFunction(() => document.querySelectorAll("#slideover-body .assignment-title").length === 4);
    const rows = await page.$$eval("#slideover-body .slide-item", (items) => items.map((n) => n.textContent));
    const row = (title) => rows.find((text) => text.includes(title)) || "";
    assert(row("Prepare the report").includes(signIn), "a job held for sign-in hides how to start it: " + JSON.stringify(row("Prepare the report")));
    assert(row("How many invoices are open?").includes(signIn), "a question held for sign-in hides how to start it: " + JSON.stringify(row("How many invoices are open?")));
    assert(row("Hire a bookkeeper").includes(longer), "the long wait hides that Stop ends it: " + JSON.stringify(row("Hire a bookkeeper")));
    // NEGATIVE CONTROL: the back end's own line is still behind Technical view.
    assert(!row("The nightly build").includes("connection"), "a back-end line shows in the ordinary view: " + JSON.stringify(row("The nightly build")));
    // Said once, not twice, in Technical view too.
    await page.evaluate(async () => {
      await window.RichBridge.invoke("set_techy_default", {enabled: true});
      await refreshTechy(activeThreadId);
      renderSlideOver();
    });
    const technical = await page.$$eval("#slideover-body .slide-item", (items) => items.map((n) => n.textContent));
    assertEqual(technical.find((text) => text.includes("Prepare the report")).split(signIn).length - 1, 1, "the sign-in line in Technical view");
    assert(technical.find((text) => text.includes("The nightly build")).includes("Opening the work connection."), "Technical view lost the back end's line");
    assertEqual(errors.length,0,"renderer errors"); await page.close();
    return "ordinary: sign-in on the held job and the held question, the long wait with Stop; the connection line hidden; Technical view: each once";
  });
  // **THE OTHER TWO THINGS THE CHIP IS BUILT FROM, AND THEY WERE NOT BEING SCOPED.**
  //
  // The check above has guarded `savedWork` across a conversation change since it was
  // written — `"old-company receipt leaked"` — and `drillItems` was cleared beside it. But
  // `renderDrillChip` reads FOUR pieces of state, and the other two, `workerCounts` and
  // `assignments.rows`, were not touched by `openThread`. So the chip went on printing the
  // conversation he had just LEFT — "1 working", "1 waiting for you", "2 assignments
  // running" — over the conversation he had just arrived at, until the three-second poll
  // came back with this thread's answer. That is one entity's work beside another entity's
  // conversation, which is the leak `openThread`'s own `closeWorkerInspector()` comment
  // calls "the exact shape of leak every guard in this build exists to stop", arriving
  // through a door nothing was watching.
  //
  // **HELD STILL RATHER THAN RACED.** The refill is on a free-running 3,000 ms interval
  // whose next tick can land ten milliseconds after the switch, so asserting "the chip is
  // clean for a moment" would be a coin toss. Instead `get_assignments` is made to HANG
  // before the switch: `pollWorkerStatus` awaits it and never returns, `workStatusBusy`
  // stays raised, and nothing can refill the chip from anything. What is on screen after
  // that is therefore exactly what `openThread` left there synchronously — which is the
  // thing under test, with the clock taken out of it.
  await run.check("the counts and the assignments do not survive the conversation they belong to", async () => {
    const page = await browser.newPage({viewport: {width:1280, height:900}});
    const errors = []; page.on("pageerror", e => errors.push(String(e)));
    await page.addInitScript(() => window.__RICHOS_MOCK_PRESET__ = {assignments: {
      hiring: [{
        id: "leak-one", title: "landing the three branches", state: "running",
        detail: "Running.", repositories: [], registeredAtMs: 1, canStop: true,
        onTheConnection: true, awaitingYou: null,
      }],
      // `acme` deliberately absent: the mock answers an unknown thread with [], which is
      // the honest shape of "this conversation has no assignments".
    }});
    await page.goto("file://" + path.join(UI_DIR,"index.html")); await leaveHome(page);
    await openThread(page,"hiring");
    // THE POSITIVE PROBE, and it has to come first: the rest of this check asserts that
    // things are ABSENT, and an absence proves nothing unless they were present.
    await page.waitForFunction(() => document.getElementById("drill-chip-zone").textContent.includes("1 assignment running"));
    const onHiring = await page.textContent("#drill-chip-zone");
    assert(onHiring.includes("1 working"), "the worker count is not on the chip to leak: " + JSON.stringify(onHiring));

    await page.evaluate(() => {
      const original = window.RichBridge.invoke.bind(window.RichBridge);
      window.RichBridge.invoke = (cmd, args) =>
        cmd === "get_assignments" ? new Promise(() => {}) : original(cmd, args);
    });
    await openThread(page,"acme");
    const onAcme = await page.textContent("#drill-chip-zone");
    assert(!onAcme.includes("assignment running"), "the previous conversation's assignment leaked: " + JSON.stringify(onAcme));
    assert(!onAcme.includes("waiting for you"), "a previous conversation's decision leaked: " + JSON.stringify(onAcme));
    assert(!onAcme.includes("working"), "the previous conversation's worker count leaked: " + JSON.stringify(onAcme));
    assert(!onAcme.includes("I can't see"), "the previous conversation's unknown-liveness count leaked: " + JSON.stringify(onAcme));
    assertEqual(errors.length,0,"renderer errors"); await page.close();
    return "on hiring: " + JSON.stringify(onHiring.trim()) + " -> on acme, with the refill held: " + JSON.stringify(onAcme.trim());
  });

  // **RAY'S CANDIDATE-.11 DEFECT 1.2 — "1 saved work records".**
  //
  // §1.2 of `docs/verification/2026-09-19-nightly-1.2.0-nightly.20260918.6-mac-and-android-audit.md`,
  // screenshots 06 and 08: a singular count with a plural noun, in the status line he reads
  // most often. The check above waits on the TWO-record form and would have passed for ever
  // with the one-record form broken, which is the whole reason this is a separate check: the
  // fixture that shows the defect is a fixture with exactly one receipt in it.
  //
  // Asserted over the WHOLE chip and not only the saved-work part, because the defect is that
  // the rule existed at three of its four sites. The chip now prints every count through one
  // `counted(n, singular, plural)`, so this check is what notices a fifth part that does not.
  await run.check("every count on the status line agrees with its own noun, at one and at more than one", async () => {
    const page = await browser.newPage({viewport: {width:1280, height:900}});
    const errors = []; page.on("pageerror", e => errors.push(String(e)));
    await page.addInitScript(() => window.__RICHOS_MOCK_PRESET__ = {workSummaries: {
      hiring: {items:[{title:"Reviewed change", role:"worker", repository:"/fictional/one", state:"integrated", detail:"Reviewed commit integrated locally. Whole assignment remains open."}], omitted:0}
    }});
    await page.goto("file://" + path.join(UI_DIR,"index.html")); await leaveHome(page);
    await openThread(page,"hiring");
    await page.waitForFunction(() => document.getElementById("drill-chip-zone").textContent.includes("team job"));
    const one = await page.textContent("#drill-chip-zone");
    assert(one.includes("1 team job"), `the chip reads ${JSON.stringify(one.trim())}`);
    assert(!one.includes("1 team jobs"), `singular count, plural noun: ${JSON.stringify(one.trim())}`);

    assertEqual(errors.length,0,"renderer errors"); await page.close();

    // POSITIVE CONTROL, on its own page: the plural must still be plural. Without it, "fix the
    // noun" could pass by printing the singular everywhere — the same defect facing the other
    // way, and a check that only ever sees one receipt cannot tell the two apart.
    const many = await browser.newPage({viewport: {width:1280, height:900}});
    await many.addInitScript(() => window.__RICHOS_MOCK_PRESET__ = {workSummaries: {
      hiring: {items:[
        {title:"Reviewed change", role:"worker", repository:"/fictional/one", state:"integrated", detail:"Reviewed commit integrated locally. Whole assignment remains open."},
        {title:"Second change", role:"worker", repository:"/fictional/two", state:"integrated", detail:"Reviewed commit integrated locally. Whole assignment remains open."}
      ], omitted:0}
    }});
    await many.goto("file://" + path.join(UI_DIR,"index.html")); await leaveHome(many);
    await openThread(many,"hiring");
    await many.waitForFunction(() => document.getElementById("drill-chip-zone").textContent.includes("team job"));
    const two = await many.textContent("#drill-chip-zone");
    assert(two.includes("2 team jobs"), `two receipts read ${JSON.stringify(two.trim())}`);
    await many.close();
    return `one receipt reads ${JSON.stringify(one.trim())}; two read ${JSON.stringify(two.trim())}`;
  });

  // **THE CHIP IS RIGHT IN THE FIRST RENDERED STATE — at launch and after a switch.**
  //
  // The two checks above prove the chip is never the PREVIOUS conversation's. This one proves
  // it is THIS conversation's straight away, which is the other half of the same defect: the
  // refill used to be a free-running 3,000 ms interval with no leading call, so after every
  // launch and every switch the status line he reads most often was empty for up to three
  // seconds while work was already running. `techy.js`'s own shutter note measured it — the
  // line absent at the shutter and present 1.6 s later.
  //
  // **THE CONTROL IS THE INIT SCRIPT, NOT THE WAIT.** A `waitForFunction` on a chip that a
  // 3,000 ms interval is about to fill would go green with or without the fix; it would time
  // nothing and prove nothing. So the interval is TAKEN AWAY before a line of the app runs,
  // and what remains is the leading `pollWorkerStatus()` in `openThread` and nothing else. If
  // that call is removed, every assertion below times out with an empty chip.
  //
  // 3,000 ms IS UNIQUE TO THE WORKER POLL in the shipping UI — `main.js` also creates a
  // 1,000 ms timer tick and a 1,000 ms wait band, `phone.js` a 2,000 ms poller and
  // `permissions.js` a 500 ms one — so the filter hits exactly one interval, and the run
  // ASSERTS that rather than assuming it. A future `WORKER_POLL_MS` that stops being 3,000
  // fails here loudly instead of quietly disarming the control.
  await run.check("the work chip carries this conversation's work in the FIRST rendered state — at launch and after a switch, with the 3,000 ms refill taken away", async () => {
    const page = await browser.newPage({viewport: {width:1280, height:900}});
    const errors = []; page.on("pageerror", e => errors.push(String(e)));
    await page.addInitScript(() => {
      window.__RICHOS_BLOCKED_INTERVALS__ = [];
      const realSetInterval = window.setInterval.bind(window);
      window.setInterval = function (fn, ms) {
        if (ms === 3000) { window.__RICHOS_BLOCKED_INTERVALS__.push(ms); return 0; }
        return realSetInterval.apply(null, arguments);
      };
      window.__RICHOS_MOCK_PRESET__ = {assignments: {
        hiring: [{
          id: "first-second", title: "landing the three branches", state: "running",
          detail: "Running.", repositories: [], registeredAtMs: 1, canStop: true,
          onTheConnection: true, awaitingYou: null,
        }],
      }};
    });

    // LAUNCH. Nothing is clicked: `init()` restores `active_thread` and opens it itself, which
    // is the path every double-clicked launch takes.
    const t0 = Date.now();
    await page.goto("file://" + path.join(UI_DIR,"index.html")); await leaveHome(page);
    const atLaunch = await chipWithin(page, "1 working", "at launch");
    const launchMs = Date.now() - t0;

    // AND AFTER A SWITCH, to a conversation whose work is its own: `hiring` has an assignment
    // and the restored thread does not, so the line that has to arrive here cannot be left
    // over from the surface before it.
    assert(!atLaunch.includes("assignment running"), "the restored thread already showed hiring's assignment: " + JSON.stringify(atLaunch));
    const t1 = Date.now();
    await openThread(page, "hiring");
    const onHiring = await chipWithin(page, "1 assignment running", "after the switch to hiring");
    const switchMs = Date.now() - t1;

    // THE CONTROL PROVED, not assumed: exactly one interval was refused, and it was the
    // worker poll's.
    assertEqual(await page.evaluate(() => window.__RICHOS_BLOCKED_INTERVALS__), [3000],
      "the 3,000 ms refill was not the only interval taken away, or was not taken away at all — the control this check rests on did not hold");

    assertEqual(errors.length,0,"renderer errors"); await page.close();
    return "with the refill blocked: launch showed " + JSON.stringify(atLaunch.trim()) + " " + launchMs +
      "ms after goto; the switch showed " + JSON.stringify(onHiring.trim()) + " " + switchMs + "ms after the press";
  });

  // **NIGHTLY 48'S WALK, D26** (docs/verification/2026-10-10-nightly-48-candidate-walk.md, frame
  // `h-held`): while the queue was held for a sign-in, the status line read "2 assignments
  // starting" right under the notice "I haven't started … yet". A held job's row is `registered`,
  // like a job about to start; the shell now says which are held (`held`, from
  // `WorkHost::held_on`), and the line says they are waiting to start.
  await run.check("held work is never called starting: the line says it is waiting to start, and work that really is starting still says so", async () => {
    const page = await browser.newPage({viewport: {width:1280, height:900}});
    const errors = []; page.on("pageerror", e => errors.push(String(e)));
    const row = (id, title, held) => ({
      id, title, state: "registered", detail: held ? "Waiting for you to sign in to Claude, in Settings under Claude accounts." : "Written down.",
      repositories: [], registeredAtMs: 1, canStop: true, onTheConnection: false, held, awaitingYou: null,
    });
    await page.addInitScript((rows) => window.__RICHOS_MOCK_PRESET__ = {assignments: rows}, {
      hiring: [row("held-1", "Prepare the Northwind invoice summary", true), row("held-2", "the request behind it", true)],
      acme: [row("fresh-1", "Draft the onboarding note", false)],
    });
    await page.goto("file://" + path.join(UI_DIR,"index.html")); await leaveHome(page);
    await openThread(page,"hiring");
    const held = await chipWithin(page, "assignment", "on hiring, held");
    assert(held.includes("2 assignments waiting to start"), "the held line reads " + JSON.stringify(held.trim()));
    assert(!held.includes("starting"), "held work is called starting: " + JSON.stringify(held.trim()));
    // POSITIVE CONTROL: a job written down and not held is still starting.
    await openThread(page,"acme");
    const fresh = await chipWithin(page, "1 assignment starting", "on acme, not held");
    assert(!fresh.includes("waiting to start"), "work that is starting is called waiting: " + JSON.stringify(fresh.trim()));
    assertEqual(errors.length,0,"renderer errors"); await page.close();
    return "held: " + JSON.stringify(held.trim()) + "; not held: " + JSON.stringify(fresh.trim());
  });

  // **NOTHING IN THE CONVERSATION HIDES UNDER "UNDER THE HOOD".** Final walk on main 0a2ae649f
  // (2026-10-14, richos-hq `docs/verification/2026-10-14-final55-walk.md` line 113): with the
  // panel open, the conversation's last card and the composer's Output button ran under the
  // panel's left edge, because the panel is `position: fixed` and the conversation kept the
  // whole window. Measured at the default window (1400x880) and the smallest (1024x700), in
  // both themes: every box in the conversation, the header and the composer row ends at or
  // before the panel's left edge, and the conversation never scrolls sideways to get there.
  await run.check("with Under the hood open, the conversation and the composer stop at the panel's edge, at 1400x880 and 1024x700, light and dark", async () => {
    const seen = [];
    for (const [width, height] of [[1400, 880], [1024, 700]]) {
      for (const colorScheme of ["light", "dark"]) {
        const where = `${width}x${height} ${colorScheme}`;
        const page = await browser.newPage({viewport: {width, height}, colorScheme});
        const errors = []; page.on("pageerror", e => errors.push(String(e)));
        await page.addInitScript(() => window.__RICHOS_MOCK_PRESET__ = {workSummaries: {
          hiring: {items:[{title:"Reviewed change", role:"worker", repository:"/fictional/project", state:"integrated", detail:"Reviewed commit integrated locally. Whole assignment remains open."}], omitted:2}
        }});
        await page.goto("file://" + path.join(UI_DIR,"index.html")); await leaveHome(page);
        await openThread(page,"hiring");
        await page.waitForFunction(() => document.getElementById("drill-chip-zone").textContent.includes("1 team job"));
        await page.click(".drill-chip");
        await page.waitForFunction(() => !document.getElementById("slideover").hidden);
        const m = await page.evaluate(() => {
          const edge = document.getElementById("slideover").getBoundingClientRect().left;
          const shown = (n) => {
            const r = n.getBoundingClientRect();
            const cs = getComputedStyle(n);
            return r.width > 1 && r.height > 1 && cs.visibility !== "hidden" && cs.display !== "none";
          };
          const over = [];
          for (const n of document.querySelectorAll("#stage-header *, #conversation *, #composer-row, #composer-row *")) {
            if (!shown(n)) continue;
            const right = n.getBoundingClientRect().right;
            if (right > edge + 0.5) over.push((n.id ? "#" + n.id : n.tagName.toLowerCase() + "." + [...n.classList].join(".")) + " ends at " + Math.round(right));
          }
          const conv = document.getElementById("conversation");
          const out = document.getElementById("out-bottom");
          return {edge: Math.round(edge), over, sideways: conv.scrollWidth - conv.clientWidth,
            outShown: !!out && !out.hidden && shown(out)};
        });
        assert(m.outShown, where + ": the composer's Output button is not on screen to measure");
        assertEqual(m.over.length, 0, where + ": ran under the panel's left edge at " + m.edge + "px: " + m.over.slice(0, 6).join("; "));
        assert(m.sideways <= 0, where + ": the conversation scrolls sideways by " + m.sideways + "px to keep inside the panel's edge");
        assertEqual(errors.length, 0, where + ": renderer errors");
        seen.push(where + " edge " + m.edge);
        await page.close();
      }
    }
    return seen.join(", ");
  });
  await browser.close(); process.exit(run.report()?1:0);
}
main().catch(e=>{console.error(e);process.exit(1);});

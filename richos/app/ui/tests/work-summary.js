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
    await page.waitForFunction(() => document.getElementById("drill-chip-zone").textContent.includes("2 saved work records"));
    await page.click(".drill-chip");
    assert((await page.textContent("#slideover-body")).includes("Review is still required"), "run end became success");
    assert((await page.textContent("#slideover-body")).includes("Whole assignment remains open"), "integration became whole-task completion");
    assertEqual(await page.locator("#slideover-body img").count(),0,"receipt rendered as markup");
    await page.click("#slideover-close"); await openThread(page,"acme");
    assert(!(await page.textContent("#drill-chip-zone")).includes("2 saved work records"),"old-company receipt leaked");
    await page.evaluate(() => window.__RICHOS_MOCK_PRESET__.workSummaryError = "Synthetic damaged receipt");
    await page.waitForFunction(() => document.getElementById("drill-chip-zone").textContent.includes("Saved work unavailable"));
    await page.click(".drill-chip");
    assert((await page.textContent("#slideover-body")).includes("Synthetic damaged receipt"),"damaged receipt looked empty");
    assertEqual(errors.length,0,"renderer errors"); await page.close();
    return "scoped receipts, explicit outcomes, escaped content and visible read failure";
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
    await page.waitForFunction(() => document.getElementById("drill-chip-zone").textContent.includes("saved work record"));
    const one = await page.textContent("#drill-chip-zone");
    assert(one.includes("1 saved work record"), `the chip reads ${JSON.stringify(one.trim())}`);
    assert(!one.includes("1 saved work records"), `singular count, plural noun: ${JSON.stringify(one.trim())}`);

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
    await many.waitForFunction(() => document.getElementById("drill-chip-zone").textContent.includes("saved work record"));
    const two = await many.textContent("#drill-chip-zone");
    assert(two.includes("2 saved work records"), `two receipts read ${JSON.stringify(two.trim())}`);
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

  await browser.close(); process.exit(run.report()?1:0);
}
main().catch(e=>{console.error(e);process.exit(1);});

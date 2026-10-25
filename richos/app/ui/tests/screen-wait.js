// A job waiting for the screen, on the timeline — against the REAL renderer under WebKit.
//
// The CEO's ruling §56 (2026-09-18): the app waits for the Mac's screen to unlock "for
// automatic use when Rich needs it", and nothing asks him to unlock. The brief that wired it
// (2026-10-25) says the waiting job "shows one plain line on the timeline and in the status
// read". The mid-job review of ed81e5e8b (rv-20261025T001333Z-ed81e5e8-29df, finding 2) found
// the timeline half missing: `fixtures/check_wait_timeline.js` printed "timeline waiting
// items=0" for a waiting-for-screen row. This suite is that fixture, as a test.
//
// FIVE PROPERTIES:
//   1. A waiting-for-screen row puts "Waiting for the screen to unlock" on the line of the turn
//      that gave the job, where "Worked for 2s" would otherwise be. No timer: it ticks nothing.
//   2. Once the screen is back (the record's `screen::says::carrying_on` detail, still
//      `registered`) the line says it is carrying on; once Stu reports on the job it is the
//      turn's own duration again (the positive control: "Worked").
//   3. It survives the next snapshot, which rebuilds every turn record.
//   4. A row from another conversation changes nothing.
//   5. It clears WCAG AA in both themes, computed from WebKit's resolved colors with the
//      shipping checker's arithmetic. Nothing here is declared exempt: it tells him why
//      nothing is moving.
//
// NO AUDIO PATH IS TOUCHED: headless WebKit, no voice mode, no output device, no playback.
//
// RUN:  node screen-wait.js

"use strict";

const { loadPlaywright, openFixture, createRun, assert, assertEqual } = require("./lib/harness");
const F = require("./lib/fixtures");
const C = require("./lib/contrast");

const WAITING_DETAIL = "The screen is locked, so this is waiting for it to unlock. It will carry on by itself.";
const BACK_DETAIL = "The screen is back, so this is carrying on.";

/// A task row in `get_assignments`' wire shape (`src-tauri/src/main.rs`).
function taskRow(state, detail, thread) {
  return {
    id: "screen-job",
    title: "Walk the build on screen",
    kind: "task",
    state,
    detail,
    repositories: [],
    registeredAtMs: 1787948000000,
    turnRef: "ledger:" + (thread || F.THREAD) + ":" + F.TURN,
    forYou: false,
    answered: false,
    canStop: true,
  };
}

/// A completed turn ("I'll take care of it.", 2 s), the register's rows, a render: the label.
function drive(page, rows, { resnapshot } = {}) {
  return page.evaluate(
    ({ rows, snapshot, resnapshot }) => {
      const model = window.RichTimeline.createModel();
      window.RichTimeline.bind(model, snapshot.entityId, snapshot.threadId, 1);
      window.RichTimeline.applySnapshot(model, snapshot);
      window.RichTimeline.setQuestionsInFlight(model, rows, snapshot.threadId);
      window.RichTimeline.setScreenWaits(model, rows, snapshot.threadId);
      if (resnapshot) window.RichTimeline.applySnapshot(model, snapshot);
      const container = document.getElementById("messages");
      window.RichTimeline.render(model, container, {
        now: 1787948010000,
        expandedMessages: new Set(),
        avatarAlreadyShown: true,
        isExpanded: () => false,
        toggle: () => {},
        rerender: () => {},
        copy: () => {},
        retry: () => {},
        openWorker: () => {},
      });
      const label = container.querySelector(".tl-duration-label");
      const control = label ? label.closest(".tl-duration-btn") : null;
      return {
        label: label ? label.textContent : null,
        ariaLabel: control ? control.getAttribute("aria-label") : null,
        ticking: window.RichTimeline.hasLiveRow(model),
      };
    },
    {
      rows,
      resnapshot: !!resnapshot,
      snapshot: F.snapshot([F.userMessage(), F.richMessage(0, "I'll take care of it.", 1), F.duration("completed", 2000)]),
    }
  );
}

async function main() {
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();
  const run = createRun("§56 a job waiting for the screen, on the timeline");
  const page = await openFixture(browser);

  await run.check("with no screen wait the line is the turn's own duration (positive control)", async () => {
    const r = await drive(page, [taskRow("running", "Stu reported it running.")]);
    assert(/^Worked/.test(r.label), "the control row did not read the turn's duration: " + r.label);
    return r.label;
  });

  await run.check('a job waiting for the screen reads "Waiting for the screen to unlock" on its turn', async () => {
    const r = await drive(page, [taskRow("waiting-for-screen", WAITING_DETAIL)]);
    assertEqual(r.label, "Waiting for the screen to unlock", "the line under the turn that gave the job");
    assert(r.ariaLabel.includes("I'll carry on the moment it's back."), "the accessible name: " + r.ariaLabel);
    assert(!/unlock your|please|you need/i.test(r.ariaLabel), "the line asks him for something: " + r.ariaLabel);
    assert(!r.ticking, "a screen wait has no timer, so nothing may keep a 1 Hz tick alive for it");
    return r.label + " · " + r.ariaLabel;
  });

  await run.check("once the screen is back the line says it is carrying on, then returns to the duration", async () => {
    const back = await drive(page, [taskRow("registered", BACK_DETAIL)]);
    assertEqual(back.label, "The screen is back, so this is carrying on", "right after the unlock");
    const reported = await drive(page, [taskRow("running", "Stu reported it running.")]);
    assert(/^Worked/.test(reported.label), "after Stu reported, the line kept a screen claim: " + reported.label);
    const plain = await drive(page, [taskRow("registered", "Written down. Preparation has not started.")]);
    assert(/^Worked/.test(plain.label), "an ordinary registered job claimed the screen: " + plain.label);
    return back.label + " -> " + reported.label;
  });

  await run.check("the waiting line survives the next snapshot", async () => {
    const r = await drive(page, [taskRow("waiting-for-screen", WAITING_DETAIL)], { resnapshot: true });
    assertEqual(r.label, "Waiting for the screen to unlock", "after applySnapshot rebuilt the turns");
    return r.label;
  });

  await run.check("a waiting row from another conversation changes nothing here", async () => {
    const r = await drive(page, [taskRow("waiting-for-screen", WAITING_DETAIL, "another-thread")]);
    assert(/^Worked/.test(r.label), "another thread's job reached this timeline: " + r.label);
    return r.label;
  });

  const LABEL_PATH = "tl-duration-label";
  for (const theme of ["dark", "light"]) {
    await run.check(`the waiting line clears WCAG AA in ${theme} mode, computed`, async () => {
      const measured = [];
      for (const [state, detail] of [["waiting-for-screen", WAITING_DETAIL], ["registered", BACK_DETAIL]]) {
        await page.evaluate((t) => document.documentElement.setAttribute("data-theme", t), theme);
        await drive(page, [taskRow(state, detail)]);
        await page.evaluate(C.pageScript());
        const out = await page.evaluate(
          (o) => {
            const probe = window.__contrastProbe(o);
            const node = document.querySelector(".tl-duration-label");
            const cs = getComputedStyle(node);
            let bg = null;
            for (let el = node; el && !bg; el = el.parentElement) {
              const c = window.__contrastMath.parseCssColor(getComputedStyle(el).backgroundColor);
              if (c && c.a === 1) bg = c;
            }
            const fg = window.__contrastMath.parseCssColor(cs.color);
            const ratio = bg && fg
              ? window.__contrastMath.round2(window.__contrastMath.contrastRatio(window.__contrastMath.compositeOver(fg, bg), bg))
              : null;
            return {
              text: node.textContent,
              px: parseFloat(cs.fontSize),
              ratio,
              fg: window.__contrastMath.hex(fg),
              bg: window.__contrastMath.hex(bg),
              failures: Object.values(probe.failures).map((f) => f.selector + " " + f.ratio + ":1 <" + f.threshold),
              unresolvable: Object.keys(probe.unresolvable),
              measured: probe.measuredPaths,
            };
          },
          { surface: "screen-wait", theme }
        );
        measured.push(out);
      }
      for (const m of measured) {
        assert(m.measured.some((p) => p.includes(LABEL_PATH)),
          `${theme} "${m.text}": the label was never MEASURED by the walk. unresolvable: ${JSON.stringify(m.unresolvable)}`);
        assert(!m.failures.some((f) => f.includes(LABEL_PATH)), `${theme} "${m.text}" failed the walk: ${JSON.stringify(m.failures)}`);
        assert(typeof m.ratio === "number" && m.ratio >= 4.5, `${theme} "${m.text}" measured ${m.ratio}:1, floor 4.5:1`);
        assert(m.px >= 14, `${theme} "${m.text}" renders at ${m.px}px, below the 14px floor (ceo-decisions §15)`);
      }
      await page.evaluate(() => document.documentElement.setAttribute("data-theme", "dark"));
      return measured.map((m) => `"${m.text}" ${m.px}px ${m.fg} on ${m.bg} = ${m.ratio}:1`).join(" · ");
    });
  }

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

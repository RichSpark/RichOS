// "I'll check." / "I'll investigate." — the timer beside his reply, against the REAL
// renderer under WebKit.
//
// The CEO's ruling §58 (`richos-hq` `wiki/ceo-decisions.md:2884-2914`, 2026-09-18), verbatim
// on the two halves this suite is about:
//
//   *"the "working..." timer should start running or maybe even better: a separate type of
//    timer for this kind of thing. So, in this case instead of "working" it would say
//    "investigating" next to the timer."*
//
//   *"a check still running at one minute flips its label to 'investigating' on its own."*
//
// WHAT THE BRIEF FOR THIS SLICE GOT WRONG, because it shaped the suite. It said the timeline
// timer "reads 'checking' or 'investigating' (not 'working', which stays for tasks)". There is
// no per-assignment timer beside a reply for a task: `Working for {d}` is the TURN timer, and
// under §55 the front desk's turn ENDS with its short reply — so that row reads `Worked for
// 2s` beside a question that is still being answered. So the check below is not "the word
// changed"; it is "a row that would have read `Worked for 2s` reads `Checking for 3s`", and
// the `working` scan is a NEGATIVE control rather than a relabeling.
//
// FOUR PROPERTIES, each a way this could quietly start lying:
//
//   1. THE WORD IS RIGHT FOR THE KIND, and "working" never appears beside a question. Read
//      off the rendered DOM, with a positive control — the same row with no question in
//      flight, which DOES say "Worked" — so a scan that can never fire is not mistaken for a
//      clean result.
//   2. THE FLIP IS MECHANICAL AND ON TIME. Bought with a clock rather than with a wait: the
//      page's `Date.now` is not touched at all, because `updateTimers` takes `nowMs` as an
//      argument, so the instant is supplied directly. Asserted at 59,999 ms (still checking)
//      and at 60,000 ms (investigating) — the boundary itself, in both directions.
//   3. THE TIMER STOPS WHEN THE ANSWER ARRIVES, and it stops because the RECORD stopped
//      saying so, not because anything here timed it out. Driven by handing
//      `setQuestionsInFlight` the register's next read with the row settled.
//   4. IT CLEARS WCAG AA IN BOTH THEMES, computed from WebKit's own resolved colors with the
//      shipping checker's arithmetic (`lib/contrast.js`), composited against the real ancestor
//      background. 4.5:1 for the label; nothing here is declared exempt — a timer telling him
//      his question is being looked into is text he is expected to read.
//
// PROVEN ABLE TO FAIL, on the check most likely to rot into a rubber stamp. With
// `CHECK_BECOMES_INVESTIGATE_MS` temporarily set to 30000, the flip check reported
// `it flipped early` and the run went red. A boundary gate nobody has watched fail is a
// boundary gate nobody should believe. The `working`/`worked` scan carries its own positive
// control in check 6 — the same row with no question in flight, which DOES say "Worked".
//
// NO AUDIO PATH IS TOUCHED: headless WebKit, no voice mode, no output device, no playback.
//
// RUN:  node question-timer.js
//       RICHOS_PLAYWRIGHT=/path/to/node_modules/playwright node question-timer.js

"use strict";

const { loadPlaywright, openFixture, createRun, assert, assertEqual } = require("./lib/harness");
const F = require("./lib/fixtures");
const C = require("./lib/contrast");

/// The register's own shape, as `get_assignments` puts it on the wire — `kind`, `state`,
/// `turnRef` and `registeredAtMs`. Copied from `src-tauri/src/main.rs`'s `get_assignments`,
/// not invented: a fixture that drifted from that command would make every check below a
/// statement about this file.
function assignmentRow(kind, state, registeredAtMs) {
  return {
    id: "question-one",
    title: "why the nightly has been red since Tuesday",
    kind,
    state,
    detail: "",
    repositories: [],
    registeredAtMs,
    turnRef: "ledger:" + F.THREAD + ":" + F.TURN,
    canStop: state === "registered" || state === "preparing" || state === "running",
    onTheConnection: state === "running",
  };
}

/// Build the model, apply a COMPLETED turn (which is what a §55/§58 reply leaves behind),
/// hand it the register's rows, render, and return the duration label.
///
/// `at` is the instant the row is rendered for. `registeredAt` is when the question was
/// written down — the register's own `registered_at_ms`, which is fsynced before the front
/// desk says "I'll check."
function drive(page, { kind, state, registeredAt, at, activeMs }) {
  return page.evaluate(
    ({ row, at, snapshot }) => {
      const model = window.RichTimeline.createModel();
      window.RichTimeline.bind(model, snapshot.entityId, snapshot.threadId, 1);
      window.RichTimeline.applySnapshot(model, snapshot);
      // The turn is COMPLETED — the front desk answered in two seconds and ended its turn.
      // This is the row that would otherwise read "Worked for 2s".
      window.RichTimeline.setQuestionsInFlight(model, row ? [row] : [], snapshot.threadId);
      const container = document.getElementById("messages");
      // A `Set` cannot cross the `page.evaluate` boundary, so the render options are built
      // HERE rather than passed in — the same shape `main.js` hands the renderer.
      window.RichTimeline.render(model, container, {
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
      // The 1 Hz tick, at the same instant: the live path and the first render must agree.
      const ticked = window.RichTimeline.updateTimers(model, container, at);
      const label = container.querySelector(".tl-duration-label");
      const control = label ? label.closest(".tl-duration-btn") : null;
      return {
        label: label ? label.textContent : null,
        ariaLabel: control ? control.getAttribute("aria-label") : null,
        title: control ? control.title : null,
        // Does the app keep a 1 Hz interval alive for this row at all? `main.js` asks exactly
        // this before starting one, so a row that renders a timer and is not ticked would
        // freeze on its first value.
        ticking: window.RichTimeline.hasLiveRow(model),
        tickWrote: ticked,
        flipAt: window.RichTimeline.CHECK_BECOMES_INVESTIGATE_MS,
      };
    },
    {
      row: kind ? assignmentRow(kind, state, registeredAt) : null,
      at,
      snapshot: F.snapshot([F.userMessage(), F.richMessage(0, "I'll check.", 0), F.duration("completed", activeMs)]),
    }
  );
}

async function main() {
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();
  const run = createRun("§58 the question timer — checking, investigating, and the flip");

  const page = await openFixture(browser);

  // =====================================================================================
  // 1. THE WORD
  // =====================================================================================

  await run.check('a question expected quickly reads "Checking for {duration}"', async () => {
    const r = await drive(page, {
      kind: "check",
      state: "running",
      registeredAt: 1787950000000,
      at: 1787950003000,
      activeMs: 2000,
    });
    assertEqual(r.label, "Checking for 3s", "the label beside his reply");
    // §58's own word, and the one it replaces. "Working" is the task timer's word and has no
    // business beside a question.
    assert(!/working/i.test(r.label), 'the question row said "working": ' + r.label);
    assert(!/worked/i.test(r.label), "the row fell back to the completed turn's own duration: " + r.label);
    // §6.4: the timer's seconds are not announced; the accessible name is read on focus, and
    // it is the sentence that claims only what is known.
    assert(
      r.ariaLabel.includes("Rich is checking this for you. You'll get the answer here."),
      "the accessible name does not say what is happening: " + r.ariaLabel
    );
    assert(!/finished|done|landed/i.test(r.ariaLabel), "a work receipt's framing reached the timer: " + r.ariaLabel);
    assert(r.ticking, "nothing would keep this row's timer ticking");
    assert(r.tickWrote, "the 1 Hz tick did not write this row");
    return "Checking for 3s, ticked live, and the word 'working' is nowhere on it";
  });

  await run.check('a question expected to take longer reads "Investigating for {duration}"', async () => {
    const r = await drive(page, {
      kind: "investigate",
      state: "running",
      registeredAt: 1787950000000,
      at: 1787950003000,
      activeMs: 2000,
    });
    assertEqual(r.label, "Investigating for 3s", "the label beside his reply");
    assert(!/working/i.test(r.label), 'the question row said "working": ' + r.label);
    assert(
      r.ariaLabel.includes("Rich is looking into this for you. You'll get the answer here."),
      "the accessible name: " + r.ariaLabel
    );
    return "Investigating for 3s at three seconds, from the kind the front desk reported";
  });

  await run.check("a question written down but not yet picked up still shows its timer", async () => {
    // `registered` is the state the register writes, before the back end has been asked. The
    // timer is beside HIS REPLY, so it runs from the moment he was answered — not from the
    // moment the back end got round to it.
    const r = await drive(page, {
      kind: "check",
      state: "registered",
      registeredAt: 1787950000000,
      at: 1787950012000,
      activeMs: 2000,
    });
    assertEqual(r.label, "Checking for 12s");
    return "a question the back end has not taken up yet still counts from when he was told";
  });

  // =====================================================================================
  // 2. THE FLIP — the boundary itself, in both directions
  // =====================================================================================

  await run.check("a check still running at one minute flips to investigating, and not before", async () => {
    const registeredAt = 1787950000000;
    // 999 ms before the minute. `formatDuration` floors, so this renders 59s.
    const before = await drive(page, {
      kind: "check",
      state: "running",
      registeredAt,
      at: registeredAt + 59999,
      activeMs: 2000,
    });
    assertEqual(before.label, "Checking for 59s", "it flipped early");
    // The boundary, to the millisecond.
    const at = await drive(page, {
      kind: "check",
      state: "running",
      registeredAt,
      at: registeredAt + 60000,
      activeMs: 2000,
    });
    assertEqual(at.label, "Investigating for 1m 0s", "it did not flip at the minute");
    // And it stays flipped.
    const after = await drive(page, {
      kind: "check",
      state: "running",
      registeredAt,
      at: registeredAt + 3 * 60000,
      activeMs: 2000,
    });
    assertEqual(after.label, "Investigating for 3m 0s");
    // **THE FRAME MATH, RE-DERIVED HERE RATHER THAN TRUSTED FROM THE SOURCE COMMENT.**
    // `main.js`'s `startOrStopTimer` uses `window.setInterval(…, 1000)`, so the label is
    // recomputed once per 1,000 ms. A check registered at t=0 therefore first renders the
    // flipped word on the first tick at or after t=60,000 ms:
    //     worst case = 60,000 + 1,000 - 1 = 60,999 ms  (999 ms late, never early)
    // `formatDuration` floors to whole seconds (60,999 ms -> "1m 0s"), so the number beside
    // the word cannot disagree with it at any point in that window.
    assertEqual(at.flipAt, 60000, "the flip threshold is not one minute");
    const worstCaseMs = at.flipAt + 1000 - 1;
    assertEqual(worstCaseMs, 60999, "re-derived worst case");
    return (
      "59,999 ms: Checking for 59s. 60,000 ms: Investigating for 1m 0s. 180,000 ms: still " +
      "investigating. 1 Hz tick -> the flip lands in [60,000, 60,999] ms, never early."
    );
  });

  await run.check('a question the front desk called "investigate" never reads "checking"', async () => {
    // The flip runs one way only. A rough estimate that guessed "slow" is not walked back
    // when the answer turns out to be fast — the record keeps what was estimated and the
    // screen keeps what he was told.
    const early = await drive(page, {
      kind: "investigate",
      state: "running",
      registeredAt: 1787950000000,
      at: 1787950000500,
      activeMs: 2000,
    });
    // Under a second there is no duration at all — §6.2's no-duration-yet row.
    assertEqual(early.label, "Investigating", "the bare word under one second");
    assert(!/check/i.test(early.label), "an investigate row read as a check: " + early.label);
    return "Investigating, bare, under a second — and it never reads as a check";
  });

  // =====================================================================================
  // 3. THE ANSWER STOPS IT — because the record stopped saying so
  // =====================================================================================

  await run.check("the timer goes away when the register stops listing the question as open", async () => {
    const registeredAt = 1787950000000;
    const open = await drive(page, {
      kind: "check",
      state: "running",
      registeredAt,
      at: registeredAt + 5000,
      activeMs: 2000,
    });
    assertEqual(open.label, "Checking for 5s");
    // The answer arrived: `work_host.rs` settles the assignment and raises the answer as a
    // notice. The next three-second register read carries `settled`, and that is the ONLY
    // thing that stops this timer — nothing here times anything out.
    const settled = await drive(page, {
      kind: "check",
      state: "settled",
      registeredAt,
      at: registeredAt + 5000,
      activeMs: 2000,
    });
    assertEqual(settled.label, "Worked for 2s", "the row did not go back to its own duration");
    assert(!settled.ticking, "the app would keep a 1 Hz interval running with nothing live");
    // And the same row with no register entry at all — a thread reloaded after the answer.
    const none = await drive(page, { kind: null, state: null, registeredAt, at: registeredAt + 5000, activeMs: 2000 });
    assertEqual(none.label, "Worked for 2s");
    // **POSITIVE CONTROL for the whole `working` scan above**: this is the row as it renders
    // without §58, and it DOES carry the completed turn's own word. So "the question row never
    // says worked/working" is a fact about the question row rather than about a scan that can
    // never match.
    assert(/worked/i.test(none.label), "the scan cannot fire at all; the checks above prove nothing");
    return "settled -> Worked for 2s, no interval kept; and the un-questioned row proves the scan can fire";
  });

  await run.check("a question from ANOTHER conversation cannot put a timer on this turn", async () => {
    const r = await page.evaluate(
      ({ snapshot }) => {
        const model = window.RichTimeline.createModel();
        window.RichTimeline.bind(model, snapshot.entityId, snapshot.threadId, 1);
        window.RichTimeline.applySnapshot(model, snapshot);
        const foreign = {
          id: "q", title: "x", kind: "investigate", state: "running", detail: "", repositories: [],
          registeredAtMs: 1787950000000,
          // Same turn id, different thread — the register is per conversation, and the
          // reference is what carries which one.
          turnRef: "ledger:thr_other:" + snapshot.items[0].turnId,
          canStop: true, onTheConnection: true,
        };
        window.RichTimeline.setQuestionsInFlight(model, [foreign], snapshot.threadId);
        return { size: model.questions.size, live: window.RichTimeline.hasLiveRow(model) };
      },
      { snapshot: F.snapshot([F.userMessage(), F.richMessage(0, "I'll investigate.", 0), F.duration("completed", 2000)]) }
    );
    assertEqual(r.size, 0, "another conversation's question was matched onto this turn");
    assert(!r.live, "and it would have started a timer");
    return "a turn id that matches and a thread that does not is refused";
  });

  // =====================================================================================
  // 4. CONTRAST — computed, both themes
  // =====================================================================================

  // The label is `.tl-duration-label` in `shortPath`'s spelling. Asserting the node was
  // MEASURED is the half that matters: the shipping walk files an unresolvable node in its own
  // bucket and passes the run, so "no failures" over a node nobody could resolve would be a
  // green gate that never looked at this label. That is this file's own header rule about a
  // count of nodes quietly skipped, applied to itself.
  const LABEL_PATH = "tl-duration-label";

  for (const theme of ["dark", "light"]) {
    await run.check(`both timer labels clear WCAG AA in ${theme} mode, computed`, async () => {
      const measured = [];
      for (const kind of ["check", "investigate"]) {
        await page.evaluate((t) => document.documentElement.setAttribute("data-theme", t), theme);
        await drive(page, {
          kind,
          state: "running",
          registeredAt: 1787950000000,
          at: 1787950003000,
          activeMs: 2000,
        });
        // The SHIPPING walk, over the surface as painted — the same probe `contrast.js` runs,
        // so the arithmetic here is the arithmetic its node-side check verified against the
        // published calculator rather than a second copy of it.
        await page.evaluate(C.pageScript());
        const out = await page.evaluate(
          (o) => {
            const probe = window.__contrastProbe(o);
            const node = document.querySelector(".tl-duration-label");
            const cs = getComputedStyle(node);
            // The ratio to PRINT, from the same math, composited against the first opaque
            // ancestor. The probe's own hit-test resolution is the gate above; this number is
            // for the record, and the two agree on this surface because nothing is painted
            // over the timeline row.
            let bg = null;
            for (let el = node; el && !bg; el = el.parentElement) {
              const c = window.__contrastMath.parseCssColor(getComputedStyle(el).backgroundColor);
              if (c && c.a === 1) bg = c;
            }
            const fg = window.__contrastMath.parseCssColor(cs.color);
            const ratio =
              bg && fg
                ? window.__contrastMath.round2(
                    window.__contrastMath.contrastRatio(window.__contrastMath.compositeOver(fg, bg), bg)
                  )
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
          { surface: "question-timer", theme }
        );
        measured.push(Object.assign({ kind }, out));
      }
      for (const m of measured) {
        assert(
          m.measured.some((p) => p.includes(LABEL_PATH)),
          `${theme}/${m.kind}: the timer label was never MEASURED by the walk — a skipped node, not a pass. ` +
            `unresolvable: ${JSON.stringify(m.unresolvable)}`
        );
        assert(
          !m.failures.some((f) => f.includes(LABEL_PATH)),
          `${theme}/${m.kind} "${m.text}" failed the shipping walk: ${JSON.stringify(m.failures)}`
        );
        // 4.5:1 — NORMAL text. This row is 14px (`main.js`'s own note on it), nowhere near
        // §1.4.3's large-text threshold, so the higher floor is the one that applies and the
        // one asserted. Nothing here is declared exempt: a timer telling him his question is
        // being looked into is text he is expected to read.
        assert(typeof m.ratio === "number", `${theme}/${m.kind}: the ratio could not be resolved for the record`);
        assert(m.ratio >= 4.5, `${theme}/${m.kind} "${m.text}" measured ${m.ratio}:1, floor 4.5:1`);
        // §15's floor for text meant to be easily read.
        assert(m.px >= 14, `${theme}/${m.kind} renders at ${m.px}px, below the 14px floor (ceo-decisions §15)`);
      }
      await page.evaluate(() => document.documentElement.setAttribute("data-theme", "dark"));
      return measured.map((m) => `${m.kind} "${m.text}" ${m.px}px ${m.fg} on ${m.bg} = ${m.ratio}:1`).join(" · ");
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
